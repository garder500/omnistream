import { useCallback, useEffect, useState } from "react";
import { Badge, Button, Card, CardHeader, Field, Input, Modal, PageHeader, PlatformChipGroup, Select, Toggle } from "../components/ui";
import { api } from "../lib/api";
import { PLATFORM_KEYS, platformKey, type PlatformKey } from "../lib/platforms";
import { useStore } from "../lib/store";
import type { AutomationAction, AutomationLogEntry, Rule, Target, Trigger } from "../lib/types";

const TRIGGER_LABELS: Record<Trigger["type"], string> = {
  message: "Un message contient…",
  follow: "Nouveau follow (selon la plateforme)",
  sub: "Nouvel abonnement",
  raid: "Raid reçu",
  timer: "Toutes les X secondes",
  stream_start: "Début du stream",
  stream_end: "Fin du stream",
};

const ACTION_LABELS: Record<AutomationAction["type"], string> = {
  send_message: "Envoyer un message",
  timeout: "Timeout l'auteur du message",
  delete_message: "Supprimer le message",
  ban: "Bannir l'auteur du message",
};

const DEFAULT_TRIGGERS: Record<Trigger["type"], Trigger> = {
  message: { type: "message", contains: "" },
  follow: { type: "follow" },
  sub: { type: "sub" },
  raid: { type: "raid", min_viewers: 1 },
  timer: { type: "timer", every_secs: 600 },
  stream_start: { type: "stream_start" },
  stream_end: { type: "stream_end" },
};

const DEFAULT_ACTIONS: Record<AutomationAction["type"], AutomationAction> = {
  send_message: { type: "send_message", target: { kind: "all" }, text: "" },
  timeout: { type: "timeout", seconds: 600 },
  delete_message: { type: "delete_message" },
  ban: { type: "ban", reason: null },
};

const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e));

export function describeTrigger(t: Trigger): string {
  switch (t.type) {
    case "message":
      return t.contains ? `Message contenant « ${t.contains} »` : "Tout message";
    case "raid":
      return t.min_viewers > 1 ? `Raid de ${t.min_viewers}+ viewers` : "Raid reçu";
    case "timer":
      return t.every_secs % 60 === 0 ? `Toutes les ${t.every_secs / 60} min` : `Toutes les ${t.every_secs} s`;
    default:
      return TRIGGER_LABELS[t.type];
  }
}

function describeAction(a: AutomationAction): string {
  switch (a.type) {
    case "send_message":
      return `${a.target.kind === "source" ? "Répondre" : "Envoyer partout"} : « ${a.text} »`;
    case "timeout":
      return `Timeout ${a.seconds} s`;
    default:
      return ACTION_LABELS[a.type];
  }
}

const blank = (): Rule => ({
  id: crypto.randomUUID(),
  name: "",
  enabled: true,
  trigger: DEFAULT_TRIGGERS.stream_start,
  action: { type: "send_message", target: { kind: "all" }, text: "Le live commence, bienvenue à tous !" },
  platforms: [],
  cooldown_secs: 0,
});

export default function Automations() {
  const { notify } = useStore();
  const [rules, setRules] = useState<Rule[]>([]);
  const [log, setLog] = useState<AutomationLogEntry[]>([]);
  const [editing, setEditing] = useState<Rule | null>(null);

  const refresh = useCallback(() => api.listRules().then(setRules).catch((e) => notify(errText(e), true)), [notify]);
  useEffect(() => {
    refresh();
  }, [refresh]);
  useEffect(() => {
    let alive = true;
    const load = () => api.automationLog().then((l) => alive && setLog(l)).catch(() => {});
    load();
    const t = setInterval(load, 4000);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, []);

  const save = async (r: Rule) => {
    try {
      await api.saveRule(r);
      setEditing(null);
      await refresh();
    } catch (e) {
      notify(errText(e), true);
    }
  };
  const remove = async (r: Rule) => {
    try {
      await api.deleteRule(r.id);
      await refresh();
    } catch (e) {
      notify(errText(e), true);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Automations"
        subtitle="« Si X alors Y » : réagissez aux messages, follows, raids, minuteurs et début/fin de stream"
        actions={<Button variant="primary" onClick={() => setEditing(blank())}>Nouvelle règle</Button>}
      />
      <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-auto">
        <Card className="flex flex-col divide-y divide-line overflow-hidden">
          {rules.length === 0 && <p className="p-8 text-center text-muted">Aucune règle. Créez-en une pour remercier vos nouveaux followers.</p>}
          {rules.map((r) => (
            <div key={r.id} className="flex flex-wrap items-center gap-4 p-4">
              <Toggle checked={r.enabled} label={`Activer ${r.name}`} onChange={(v) => save({ ...r, enabled: v })} />
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2.5">
                  <b className="font-display text-base">{r.name}</b>
                  <Badge tone="info">Si {describeTrigger(r.trigger)}</Badge>
                  {r.cooldown_secs > 0 && <Badge>Cooldown {r.cooldown_secs}s</Badge>}
                  {r.platforms.length > 0 && <Badge>{r.platforms.map((p) => platformKey(p)).join(", ")}</Badge>}
                </div>
                <p className="mt-1.5 truncate text-soft">Alors : {describeAction(r.action)}</p>
              </div>
              <Button variant="outline" onClick={() => setEditing(r)}>Modifier</Button>
              <Button variant="ghost" onClick={() => remove(r)}>Supprimer</Button>
            </div>
          ))}
        </Card>

        <Card>
          <CardHeader title="Journal d'exécution" aside={`${log.length} entrées`} />
          <div className="flex flex-col p-2.5">
            {log.length === 0 && <p className="p-4 text-muted">Rien n'a été déclenché pour l'instant.</p>}
            {log.slice(0, 12).map((l, i) => (
              <div key={i} className="flex flex-wrap items-center gap-3 px-3 py-2">
                <span className="text-muted">{new Date(l.at_ms).toLocaleTimeString("fr-FR")}</span>
                <Badge tone={l.error ? "danger" : "success"}>{l.origin}</Badge>
                <span className={l.error ? "text-danger-text" : "text-soft"}>{l.error ? `${l.summary} — ${l.error}` : l.summary}</span>
              </div>
            ))}
          </div>
        </Card>
      </div>
      <Modal open={editing !== null} title={editing && rules.some((r) => r.id === editing.id) ? "Modifier la règle" : "Nouvelle règle"} onClose={() => setEditing(null)}>
        {editing && <RuleForm key={editing.id} initial={editing} onCancel={() => setEditing(null)} onSave={save} />}
      </Modal>
    </div>
  );
}

function RuleForm({ initial, onSave, onCancel }: { initial: Rule; onSave: (r: Rule) => void; onCancel: () => void }) {
  const [r, setR] = useState(initial);
  const { trigger, action } = r;
  const selected = r.platforms.map(platformKey);
  const toggle = (p: PlatformKey) => setR({ ...r, platforms: selected.includes(p) ? selected.filter((x) => x !== p) : [...selected, p] });
  const num = (v: string) => Math.max(0, Math.floor(Number(v) || 0));
  const needsMessage = trigger.type === "message";
  const moderation = action.type !== "send_message";
  const invalid =
    r.name.trim() === "" ||
    (action.type === "send_message" && action.text.trim() === "") ||
    (trigger.type === "timer" && trigger.every_secs < 1) ||
    (moderation && !needsMessage);

  const changeTrigger = (type: Trigger["type"]) => {
    const next: Rule = { ...r, trigger: DEFAULT_TRIGGERS[type] };
    // La modération ne s'applique qu'à un message : on repasse à l'envoi sinon.
    if (type !== "message" && next.action.type !== "send_message") next.action = DEFAULT_ACTIONS.send_message;
    setR(next);
  };
  const setTarget = (target: Target) => action.type === "send_message" && setR({ ...r, action: { ...action, target } });

  return (
    <form
      className="flex max-h-[70vh] flex-col gap-5 overflow-auto pr-1"
      onSubmit={(e) => {
        e.preventDefault();
        onSave(r);
      }}
    >
      <Field label="Nom de la règle">
        <Input value={r.name} onChange={(e) => setR({ ...r, name: e.target.value })} placeholder="Merci pour le follow" autoFocus />
      </Field>

      <Field label="Si…">
        <Select value={trigger.type} onChange={(e) => changeTrigger(e.target.value as Trigger["type"])}>
          {(Object.keys(TRIGGER_LABELS) as Trigger["type"][]).map((t) => (
            <option key={t} value={t}>{TRIGGER_LABELS[t]}</option>
          ))}
        </Select>
      </Field>
      {trigger.type === "message" && (
        <Field label="Texte à repérer" hint="Insensible à la casse ; vide = tous les messages">
          <Input value={trigger.contains} onChange={(e) => setR({ ...r, trigger: { type: "message", contains: e.target.value } })} />
        </Field>
      )}
      {trigger.type === "raid" && (
        <Field label="Viewers minimum">
          <Input type="number" min={0} value={trigger.min_viewers} onChange={(e) => setR({ ...r, trigger: { type: "raid", min_viewers: num(e.target.value) } })} />
        </Field>
      )}
      {trigger.type === "timer" && (
        <Field label="Intervalle (secondes)">
          <Input type="number" min={1} value={trigger.every_secs} onChange={(e) => setR({ ...r, trigger: { type: "timer", every_secs: num(e.target.value) } })} />
        </Field>
      )}

      <Field label="Alors…" error={moderation && !needsMessage ? "La modération nécessite le déclencheur « message »." : undefined}>
        <Select value={action.type} onChange={(e) => setR({ ...r, action: DEFAULT_ACTIONS[e.target.value as AutomationAction["type"]] })}>
          {(Object.keys(ACTION_LABELS) as AutomationAction["type"][]).map((a) => (
            <option key={a} value={a} disabled={a !== "send_message" && !needsMessage}>{ACTION_LABELS[a]}</option>
          ))}
        </Select>
      </Field>
      {action.type === "send_message" && (
        <>
          <Field label="Message" hint="Variables : {user} {platform} {viewers} {months}">
            <Input value={action.text} onChange={(e) => setR({ ...r, action: { ...action, text: e.target.value } })} placeholder="Merci {user} !" />
          </Field>
          <Field label="Envoyer sur">
            <Select value={action.target.kind} onChange={(e) => e.target.value !== "platforms" && setTarget({ kind: e.target.value as "source" | "all" })}>
              {action.target.kind === "platforms" && <option value="platforms">Plateformes choisies (conservé)</option>}
              <option value="all">Toutes les plateformes</option>
              <option value="source">La chaîne qui a déclenché la règle</option>
            </Select>
          </Field>
        </>
      )}
      {action.type === "timeout" && (
        <Field label="Durée (secondes)">
          <Input type="number" min={1} value={action.seconds} onChange={(e) => setR({ ...r, action: { type: "timeout", seconds: Math.max(1, num(e.target.value)) } })} />
        </Field>
      )}
      {action.type === "ban" && (
        <Field label="Raison (optionnelle)">
          <Input value={action.reason ?? ""} onChange={(e) => setR({ ...r, action: { type: "ban", reason: e.target.value || null } })} />
        </Field>
      )}

      <PlatformChipGroup legend="Se déclenche sur (aucune sélection = toutes)" platforms={PLATFORM_KEYS} selected={selected} onToggle={toggle} />
      {trigger.type !== "timer" && (
        <Field label="Cooldown (secondes)" hint="Délai minimal entre deux exécutions">
          <Input type="number" min={0} value={r.cooldown_secs} onChange={(e) => setR({ ...r, cooldown_secs: num(e.target.value) })} />
        </Field>
      )}
      <div className="flex justify-end gap-3 pt-1">
        <Button type="button" variant="outline" onClick={onCancel}>Annuler</Button>
        <Button type="submit" variant="primary" disabled={invalid}>Enregistrer</Button>
      </div>
    </form>
  );
}
