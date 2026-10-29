import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/api";
import { PLATFORM_KEYS, PlatformBadge, type PlatformKey } from "../lib/platforms";
import { useStore } from "../lib/store";
import type { AutomodRule, AutomodVerdict, RuleAction, RuleKind } from "../lib/types";
import { Button, Card, Field, Input, PlatformChip, Select, Toggle } from "./ui";

const KIND_LABELS: Record<RuleKind["type"], string> = {
  regex: "Expression régulière",
  banned_words: "Mots bannis",
  links: "Liens (liste blanche)",
  caps: "Majuscules",
  repeat: "Répétitions",
  cross_platform_spam: "Spam inter-plateformes",
};

const ACTION_LABELS: Record<RuleAction["type"], string> = {
  flag: "Signaler",
  delete: "Supprimer",
  timeout: "Timeout",
};

export function defaultKind(type: RuleKind["type"]): RuleKind {
  switch (type) {
    case "regex":
      return { type, pattern: "" };
    case "banned_words":
      return { type, words: [] };
    case "links":
      return { type, allowed_domains: [] };
    case "caps":
      return { type, min_letters: 12, ratio_percent: 80 };
    case "repeat":
      return { type, min_words: 4, char_run: 15 };
    case "cross_platform_spam":
      return { type, window_secs: 60, min_channels: 2 };
  }
}

export function describeRule(r: AutomodRule): string {
  const k = r.kind;
  switch (k.type) {
    case "regex":
      return `/${k.pattern}/`;
    case "banned_words":
      return k.words.join(", ");
    case "links":
      return k.allowed_domains.length ? `Autorisés : ${k.allowed_domains.join(", ")}` : "Tous les liens";
    case "caps":
      return `≥ ${k.ratio_percent} % de majuscules (min. ${k.min_letters} lettres)`;
    case "repeat":
      return `Mot répété ${k.min_words}× ou caractère ×${k.char_run}`;
    case "cross_platform_spam":
      return `Même message sur ${k.min_channels} chaînes en ${k.window_secs} s`;
  }
}

export function actionLabel(a: RuleAction): string {
  return a.type === "timeout" ? `Timeout ${a.seconds} s` : ACTION_LABELS[a.type];
}

const splitList = (s: string) =>
  s
    .split(/[\n,]/)
    .map((x) => x.trim())
    .filter(Boolean);

function newRule(): AutomodRule {
  return {
    id: `rule-${Date.now().toString(36)}`,
    name: "",
    enabled: true,
    score: 20,
    kind: defaultKind("banned_words"),
    action: { type: "flag" },
    platforms: [],
  };
}

const num = (v: string, min = 0) => Math.max(min, Number.parseInt(v, 10) || 0);

function KindFields({ kind, onChange }: { kind: RuleKind; onChange: (k: RuleKind) => void }) {
  switch (kind.type) {
    case "regex":
      return (
        <Field label="Expression régulière" hint="Syntaxe Rust, insensible à la casse.">
          <Input value={kind.pattern} onChange={(e) => onChange({ ...kind, pattern: e.target.value })} placeholder="gagn(e|ez) \d+ ?€" />
        </Field>
      );
    case "banned_words":
      return (
        <Field label="Mots ou expressions bannis" hint="Un par ligne ou séparés par des virgules. Casse et accents ignorés.">
          <textarea
            className="min-h-24 w-full rounded-control border border-line-strong bg-field px-4 py-2.5 text-[15px] outline-none focus:border-brand focus:ring-4 focus:ring-brand/20"
            value={kind.words.join("\n")}
            onChange={(e) => onChange({ ...kind, words: splitList(e.target.value) })}
          />
        </Field>
      );
    case "links":
      return (
        <Field label="Domaines autorisés" hint="Les sous-domaines sont acceptés. Tout autre lien déclenche la règle.">
          <Input
            value={kind.allowed_domains.join(", ")}
            onChange={(e) => onChange({ ...kind, allowed_domains: splitList(e.target.value) })}
            placeholder="twitch.tv, youtube.com"
          />
        </Field>
      );
    case "caps":
      return (
        <div className="grid grid-cols-2 gap-3">
          <Field label="Part de majuscules (%)">
            <Input type="number" min={1} max={100} value={kind.ratio_percent} onChange={(e) => onChange({ ...kind, ratio_percent: num(e.target.value) })} />
          </Field>
          <Field label="Lettres minimum">
            <Input type="number" min={1} value={kind.min_letters} onChange={(e) => onChange({ ...kind, min_letters: num(e.target.value) })} />
          </Field>
        </div>
      );
    case "repeat":
      return (
        <div className="grid grid-cols-2 gap-3">
          <Field label="Mot répété (fois)">
            <Input type="number" min={0} value={kind.min_words} onChange={(e) => onChange({ ...kind, min_words: num(e.target.value) })} />
          </Field>
          <Field label="Caractère étiré (fois)">
            <Input type="number" min={0} value={kind.char_run} onChange={(e) => onChange({ ...kind, char_run: num(e.target.value) })} />
          </Field>
        </div>
      );
    case "cross_platform_spam":
      return (
        <div className="grid grid-cols-2 gap-3">
          <Field label="Chaînes différentes">
            <Input type="number" min={2} value={kind.min_channels} onChange={(e) => onChange({ ...kind, min_channels: num(e.target.value) })} />
          </Field>
          <Field label="Fenêtre (secondes)">
            <Input type="number" min={1} value={kind.window_secs} onChange={(e) => onChange({ ...kind, window_secs: num(e.target.value) })} />
          </Field>
        </div>
      );
  }
}

function RuleEditor({ rule, onSave, onCancel }: { rule: AutomodRule; onSave: (r: AutomodRule) => Promise<boolean>; onCancel: () => void }) {
  const [draft, setDraft] = useState(rule);
  const [busy, setBusy] = useState(false);
  const set = (patch: Partial<AutomodRule>) => setDraft((d) => ({ ...d, ...patch }));
  const togglePlatform = (p: PlatformKey) =>
    set({ platforms: draft.platforms.includes(p) ? draft.platforms.filter((x) => x !== p) : [...draft.platforms, p] });

  return (
    <Card className="flex w-105 shrink-0 flex-col gap-4 overflow-auto p-5">
      <h2 className="text-xl font-bold">{rule.name ? "Modifier la règle" : "Nouvelle règle"}</h2>
      <Field label="Nom">
        <Input value={draft.name} onChange={(e) => set({ name: e.target.value })} placeholder="Ex. Insultes" />
      </Field>
      <Field label="Type de règle">
        <Select value={draft.kind.type} onChange={(e) => set({ kind: defaultKind(e.target.value as RuleKind["type"]) })}>
          {(Object.keys(KIND_LABELS) as RuleKind["type"][]).map((t) => (
            <option key={t} value={t}>
              {KIND_LABELS[t]}
            </option>
          ))}
        </Select>
      </Field>
      <KindFields kind={draft.kind} onChange={(kind) => set({ kind })} />
      <div className="grid grid-cols-2 gap-3">
        <Field label="Action">
          <Select
            value={draft.action.type}
            onChange={(e) => {
              const t = e.target.value as RuleAction["type"];
              set({ action: t === "timeout" ? { type: t, seconds: 600 } : { type: t } });
            }}
          >
            {(Object.keys(ACTION_LABELS) as RuleAction["type"][]).map((t) => (
              <option key={t} value={t}>
                {ACTION_LABELS[t]}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Score" hint="Points ajoutés au message">
          <Input type="number" min={0} value={draft.score} onChange={(e) => set({ score: num(e.target.value) })} />
        </Field>
      </div>
      {draft.action.type === "timeout" && (
        <Field label="Durée du timeout (secondes)">
          <Input type="number" min={1} value={draft.action.seconds} onChange={(e) => set({ action: { type: "timeout", seconds: num(e.target.value, 1) } })} />
        </Field>
      )}
      <div>
        <div className="mb-2 text-sm font-medium text-zinc-300">Plateformes · {draft.platforms.length === 0 ? "toutes" : draft.platforms.length}</div>
        <div className="flex flex-wrap gap-2">
          {PLATFORM_KEYS.map((p) => (
            <PlatformChip key={p} platform={p} selected={draft.platforms.includes(p)} onToggle={() => togglePlatform(p)} />
          ))}
        </div>
      </div>
      <div className="mt-auto grid grid-cols-2 gap-2.5">
        <Button variant="outline" onClick={onCancel}>
          Annuler
        </Button>
        <Button
          variant="primary"
          disabled={busy}
          onClick={async () => {
            setBusy(true);
            await onSave({ ...draft, name: draft.name.trim() });
            setBusy(false);
          }}
        >
          Enregistrer
        </Button>
      </div>
    </Card>
  );
}

function TestBox() {
  const { notify } = useStore();
  const [text, setText] = useState("");
  const [platform, setPlatform] = useState<PlatformKey>("twitch");
  const [verdict, setVerdict] = useState<AutomodVerdict>();

  const run = async () => {
    try {
      setVerdict(await api.testAutomod(text, platform));
    } catch (e) {
      notify(String(e), true);
    }
  };

  return (
    <div className="border-t border-line p-4">
      <div className="mb-2 font-semibold">Tester un message</div>
      <div className="flex gap-2">
        <Input aria-label="Message à tester" value={text} onChange={(e) => setText(e.target.value)} placeholder="Collez un message pour voir quelles règles se déclenchent" />
        <Select aria-label="Plateforme du test" className="w-40" value={platform} onChange={(e) => setPlatform(e.target.value as PlatformKey)}>
          {PLATFORM_KEYS.map((p) => (
            <option key={p} value={p}>
              {p}
            </option>
          ))}
        </Select>
        <Button variant="outline" onClick={run}>
          Tester
        </Button>
      </div>
      {verdict && (
        <p className="mt-2 text-sm text-soft">
          {verdict.matches.length === 0
            ? "Aucune règle ne se déclenche."
            : `Score ${verdict.score} · ${verdict.matches.map((m) => `${m.name} (${actionLabel(m.action)})`).join(", ")}`}
        </p>
      )}
    </div>
  );
}

export default function AutomodRules() {
  const { notify } = useStore();
  const [rules, setRules] = useState<AutomodRule[]>([]);
  const [editing, setEditing] = useState<AutomodRule>();

  const refresh = useCallback(async () => {
    try {
      setRules(await api.listAutomodRules());
    } catch (e) {
      notify(String(e), true);
    }
  }, [notify]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const save = async (rule: AutomodRule) => {
    try {
      await api.saveAutomodRule(rule);
      await refresh();
      return true;
    } catch (e) {
      notify(typeof e === "string" ? e : "Règle invalide", true);
      return false;
    }
  };

  return (
    <div className="flex min-h-0 flex-1 gap-5">
      <Card className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <div className="flex items-center justify-between border-b border-line p-3.5">
          <span className="font-semibold">Règles · {rules.length}</span>
          <Button variant="primary" onClick={() => setEditing(newRule())}>
            Nouvelle règle
          </Button>
        </div>
        <div className="flex flex-col gap-2 overflow-auto p-2.5">
          {rules.length === 0 && <p className="p-6 text-center text-muted">Aucune règle : rien n'est signalé automatiquement.</p>}
          {rules.map((r) => (
            <div key={r.id} className={`flex items-center gap-3 rounded-control border border-line bg-panel p-4 ${r.enabled ? "" : "opacity-60"}`}>
              <Toggle checked={r.enabled} label={`Activer ${r.name}`} onChange={(enabled) => void save({ ...r, enabled })} />
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <b className="truncate">{r.name}</b>
                  <span className="rounded-md bg-warn-soft px-2 py-0.5 text-xs font-semibold text-warn">{actionLabel(r.action)}</span>
                  <span className="text-xs text-muted">score {r.score}</span>
                </div>
                <div className="mt-1 truncate text-sm text-muted">
                  {KIND_LABELS[r.kind.type]} · {describeRule(r)}
                </div>
              </div>
              <div className="flex shrink-0 items-center gap-1">
                {(r.platforms.length ? r.platforms : PLATFORM_KEYS).map((p) => (
                  <PlatformBadge key={p} platform={p as PlatformKey} size={20} />
                ))}
              </div>
              <Button variant="outline" onClick={() => setEditing(r)}>
                Modifier
              </Button>
              <Button
                variant="ghost"
                onClick={async () => {
                  try {
                    await api.deleteAutomodRule(r.id);
                    if (editing?.id === r.id) setEditing(undefined);
                    await refresh();
                  } catch (e) {
                    notify(String(e), true);
                  }
                }}
              >
                Supprimer
              </Button>
            </div>
          ))}
        </div>
        <div className="mt-auto">
          <TestBox />
        </div>
      </Card>
      {editing && (
        <RuleEditor
          key={editing.id}
          rule={editing}
          onCancel={() => setEditing(undefined)}
          onSave={async (r) => {
            const ok = await save(r);
            if (ok) setEditing(undefined);
            return ok;
          }}
        />
      )}
    </div>
  );
}
