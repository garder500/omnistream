import { useCallback, useEffect, useState } from "react";
import { Badge, Button, Card, Field, Input, Modal, PageHeader, PlatformChipGroup, Select, Toggle } from "../components/ui";
import { api } from "../lib/api";
import { PLATFORM_KEYS, platformKey, PLATFORMS, type PlatformKey } from "../lib/platforms";
import { useStore } from "../lib/store";
import type { ChatCommand, Role } from "../lib/types";

export const ROLE_LABELS: Record<Role, string> = {
  everyone: "Tout le monde",
  subscriber: "Abonnés et +",
  moderator: "Modérateurs et +",
  broadcaster: "Streamer uniquement",
};

const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e));

const blank = (): ChatCommand => ({
  id: crypto.randomUUID(),
  name: "",
  response: "",
  enabled: true,
  cooldown_secs: 10,
  user_cooldown_secs: 30,
  min_role: "everyone",
  platforms: [],
  reply: { kind: "source" },
});

export default function Commands() {
  const { notify } = useStore();
  const [commands, setCommands] = useState<ChatCommand[]>([]);
  const [editing, setEditing] = useState<ChatCommand | null>(null);

  const refresh = useCallback(() => api.listChatCommands().then(setCommands).catch((e) => notify(errText(e), true)), [notify]);
  useEffect(() => {
    refresh();
  }, [refresh]);

  const save = async (c: ChatCommand) => {
    try {
      await api.saveChatCommand(c);
      setEditing(null);
      await refresh();
    } catch (e) {
      notify(errText(e), true);
    }
  };
  const remove = async (c: ChatCommand) => {
    try {
      await api.deleteChatCommand(c.id);
      await refresh();
    } catch (e) {
      notify(errText(e), true);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Commandes"
        subtitle="Définies une seule fois, elles répondent sur toutes vos plateformes avec cooldowns et permissions"
        actions={<Button variant="primary" onClick={() => setEditing(blank())}>Nouvelle commande</Button>}
      />
      <Card className="flex flex-col divide-y divide-line overflow-hidden">
        {commands.length === 0 && <p className="p-8 text-center text-muted">Aucune commande. Créez par exemple !discord ou !so.</p>}
        {commands.map((c) => (
          <div key={c.id} className="flex flex-wrap items-center gap-4 p-4">
            <Toggle checked={c.enabled} label={`Activer !${c.name}`} onChange={(v) => save({ ...c, enabled: v })} />
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2.5">
                <b className="font-display text-base">!{c.name}</b>
                <Badge tone="brand">{ROLE_LABELS[c.min_role]}</Badge>
                {(c.cooldown_secs > 0 || c.user_cooldown_secs > 0) && (
                  <Badge>
                    Cooldown {c.cooldown_secs}s · {c.user_cooldown_secs}s/pers.
                  </Badge>
                )}
                <Badge tone="info">{c.platforms.length === 0 ? "Toutes plateformes" : c.platforms.map((p) => PLATFORMS[platformKey(p)].label).join(", ")}</Badge>
                {c.reply.kind === "all" && <Badge tone="warn">Répond partout</Badge>}
              </div>
              <p className="mt-1.5 truncate text-soft">{c.response}</p>
            </div>
            <Button variant="outline" onClick={() => setEditing(c)}>Modifier</Button>
            <Button variant="ghost" onClick={() => remove(c)}>Supprimer</Button>
          </div>
        ))}
      </Card>
      <Modal open={editing !== null} title={editing && commands.some((c) => c.id === editing.id) ? "Modifier la commande" : "Nouvelle commande"} onClose={() => setEditing(null)}>
        {editing && <CommandForm key={editing.id} initial={editing} onCancel={() => setEditing(null)} onSave={save} />}
      </Modal>
    </div>
  );
}

function CommandForm({ initial, onSave, onCancel }: { initial: ChatCommand; onSave: (c: ChatCommand) => void; onCancel: () => void }) {
  const [c, setC] = useState(initial);
  const set = <K extends keyof ChatCommand>(k: K, v: ChatCommand[K]) => setC((x) => ({ ...x, [k]: v }));
  const selected = c.platforms.map(platformKey);
  const toggle = (p: PlatformKey) => set("platforms", selected.includes(p) ? selected.filter((x) => x !== p) : [...selected, p]);
  const num = (v: string) => Math.max(0, Math.floor(Number(v) || 0));
  const invalid = !/^!?[\p{L}\p{N}_]+$/u.test(c.name.trim()) || c.response.trim() === "";

  return (
    <form
      className="flex max-h-[70vh] flex-col gap-5 overflow-auto pr-1"
      onSubmit={(e) => {
        e.preventDefault();
        onSave({ ...c, name: c.name.trim().replace(/^!/, "").toLowerCase() });
      }}
    >
      <Field label="Nom de la commande" hint="Lettres, chiffres et _ ; sans le « ! »">
        <Input value={c.name} onChange={(e) => set("name", e.target.value)} placeholder="discord" autoFocus />
      </Field>
      <Field label="Réponse" hint="Variables : {user} {platform} {args} {arg1}…{arg5}">
        <Input value={c.response} onChange={(e) => set("response", e.target.value)} placeholder="Rejoins-nous sur discord.gg/omnistream" />
      </Field>
      <Field label="Qui peut l'utiliser ?">
        <Select value={c.min_role} onChange={(e) => set("min_role", e.target.value as Role)}>
          {(Object.keys(ROLE_LABELS) as Role[]).map((r) => (
            <option key={r} value={r}>{ROLE_LABELS[r]}</option>
          ))}
        </Select>
      </Field>
      <div className="grid grid-cols-2 gap-4">
        <Field label="Cooldown global (s)">
          <Input type="number" min={0} value={c.cooldown_secs} onChange={(e) => set("cooldown_secs", num(e.target.value))} />
        </Field>
        <Field label="Cooldown par personne (s)">
          <Input type="number" min={0} value={c.user_cooldown_secs} onChange={(e) => set("user_cooldown_secs", num(e.target.value))} />
        </Field>
      </div>
      <PlatformChipGroup legend="Active sur (aucune sélection = toutes)" platforms={PLATFORM_KEYS} selected={selected} onToggle={toggle} />
      <Field label="Où répondre ?">
        <Select value={c.reply.kind} onChange={(e) => e.target.value !== "platforms" && set("reply", { kind: e.target.value as "source" | "all" })}>
          {c.reply.kind === "platforms" && <option value="platforms">Plateformes choisies (conservé)</option>}
          <option value="source">Là où la commande a été tapée</option>
          <option value="all">Sur toutes les plateformes</option>
        </Select>
      </Field>
      <div className="flex justify-end gap-3 pt-1">
        <Button type="button" variant="outline" onClick={onCancel}>Annuler</Button>
        <Button type="submit" variant="primary" disabled={invalid}>Enregistrer</Button>
      </div>
    </form>
  );
}
