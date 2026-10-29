import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/api";
import { PLATFORM_KEYS, PLATFORMS, PlatformBadge, platformKey } from "../lib/platforms";
import { useStore } from "../lib/store";
import type { BanJob, IdentityLink, Platform, PreviewItem, SyncKind, SyncRequest } from "../lib/types";
import { Badge, Button, Card, Checkbox, Field, Input, Select } from "./ui";

export interface BanSyncPrefill {
  platform: Platform;
  userId: string;
  name: string;
}

const slug = (p: Platform) => (typeof p === "string" ? p : p.other);
const toPlatform = (s: string): Platform => ((PLATFORM_KEYS as string[]).includes(s) ? (s as Platform) : { other: s });
const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e));
const STATUS_TONE = { done: "success", failed: "danger", pending: "warn" } as const;
const STATUS_LABEL = { done: "Appliqué", failed: "Échec", pending: "En attente" } as const;

/** Ban / unban propagé sur toutes les plateformes via la table d'identités. */
export function BanSyncPanel({ prefill }: { prefill?: BanSyncPrefill }) {
  const { notify } = useStore();
  // Slug brut de la plateforme : ne jamais ramener une plateforme inconnue à Twitch.
  const [platform, setPlatform] = useState<string>(prefill ? slug(prefill.platform) : "twitch");
  const [userId, setUserId] = useState(prefill?.userId ?? "");
  const [name, setName] = useState(prefill?.name ?? "");
  const [kind, setKind] = useState<SyncKind>("ban");
  const [reason, setReason] = useState("");
  const [preview, setPreview] = useState<PreviewItem[]>();
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [jobs, setJobs] = useState<BanJob[]>([]);
  const [links, setLinks] = useState<IdentityLink[]>([]);
  const [busy, setBusy] = useState(false);
  const [newLink, setNewLink] = useState({ person: "", platform: "twitch", user_id: "", display_name: "" });

  const refresh = useCallback(() => {
    api.bansyncJournal().then(setJobs).catch((e) => notify(errText(e), true));
    api.bansyncIdentities().then(setLinks).catch((e) => notify(errText(e), true));
  }, [notify]);
  useEffect(refresh, [refresh]);

  const request = (channelIds: string[] | null): SyncRequest => ({
    source_platform: toPlatform(platform),
    source_user_id: userId.trim(),
    source_name: name.trim() || userId.trim(),
    kind,
    reason: reason.trim() || null,
    channel_ids: channelIds,
  });

  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    try {
      await fn();
    } catch (e) {
      notify(errText(e), true);
    } finally {
      setBusy(false);
    }
  };

  const doPreview = () =>
    run(async () => {
      const items = await api.bansyncPreview(request(null));
      setPreview(items);
      setSelected(new Set(items.filter((i) => i.status === "ready").map((i) => i.channel.id)));
    });

  const apply = () =>
    run(async () => {
      setJobs(await api.bansyncSubmit(request([...selected])));
      setPreview(undefined);
      notify(kind === "ban" ? "Ban synchronisé envoyé" : "Débannissement synchronisé envoyé");
    });

  const retry = (jobId: string | null) => run(async () => setJobs(await api.bansyncRetry(jobId)));

  const addLink = () =>
    run(async () => {
      await api.bansyncLink({ ...newLink, platform: toPlatform(newLink.platform) });
      setNewLink({ ...newLink, user_id: "", display_name: "" });
      refresh();
    });

  const persons = [...new Set(links.map((l) => l.person))];

  return (
    <div className="flex min-h-0 flex-1 gap-5 overflow-auto">
      <div className="flex min-w-0 flex-1 flex-col gap-5">
        <Card className="flex flex-col gap-4 p-5">
          <h2 className="font-display text-base font-semibold">Ban synchronisé</h2>
          <div className="grid grid-cols-2 gap-3.5">
            <Field label="Plateforme d'origine">
              <Select value={platform} onChange={(e) => setPlatform(e.target.value)}>
                {PLATFORM_KEYS.map((k) => (
                  <option key={k} value={k}>
                    {PLATFORMS[k].label}
                  </option>
                ))}
                {!(PLATFORM_KEYS as string[]).includes(platform) && <option value={platform}>{platform}</option>}
              </Select>
            </Field>
            <Field label="Action">
              <Select value={kind} onChange={(e) => setKind(e.target.value as SyncKind)}>
                <option value="ban">Bannir</option>
                <option value="unban">Débannir</option>
              </Select>
            </Field>
            <Field label="Identifiant sur la plateforme">
              <Input value={userId} onChange={(e) => setUserId(e.target.value)} placeholder="ex. 12345678" />
            </Field>
            <Field label="Pseudo (affichage)">
              <Input value={name} onChange={(e) => setName(e.target.value)} />
            </Field>
          </div>
          <Field label="Motif" hint="Transmis aux plateformes qui le supportent.">
            <Input value={reason} onChange={(e) => setReason(e.target.value)} />
          </Field>
          <Button variant="primary" disabled={busy || !userId.trim()} onClick={doPreview}>
            Prévisualiser les chaînes ciblées
          </Button>

          {preview && (
            <div className="flex flex-col gap-1" aria-label="Prévisualisation">
              {preview.length === 0 && <p className="text-muted">Aucune chaîne suivie.</p>}
              {preview.map((i) => {
                const ready = i.status === "ready";
                return (
                  <div key={i.channel.id} className="flex items-center gap-2">
                    <div className="flex-1">
                      <Checkbox
                        checked={ready && selected.has(i.channel.id)}
                        onChange={(v) => {
                          const next = new Set(selected);
                          if (v) next.add(i.channel.id);
                          else next.delete(i.channel.id);
                          setSelected(next);
                        }}
                      >
                        <span className={`inline-flex items-center gap-2 ${ready ? "" : "opacity-50"}`}>
                          <PlatformBadge platform={platformKey(i.channel.platform)} size={20} />#{i.channel.display_name}
                        </span>
                      </Checkbox>
                    </div>
                    {ready ? (
                      <span className="text-xs text-muted">ID {i.user_ids.join(", ")}</span>
                    ) : (
                      <Badge tone="warn">Identité inconnue</Badge>
                    )}
                  </div>
                );
              })}
              <Button variant="danger" className="mt-2" disabled={busy || selected.size === 0} onClick={apply}>
                {kind === "ban" ? "Bannir" : "Débannir"} sur {selected.size} chaîne{selected.size > 1 ? "s" : ""}
              </Button>
            </div>
          )}
        </Card>

        <Card className="flex flex-col gap-3 p-5">
          <div className="flex items-center justify-between gap-3">
            <h2 className="font-display text-base font-semibold">Journal des bans synchronisés</h2>
            {jobs.some((j) => j.items.some((i) => i.status === "failed")) && (
              <Button variant="warn" disabled={busy} onClick={() => retry(null)}>
                Rejouer tous les échecs
              </Button>
            )}
          </div>
          {jobs.length === 0 && <p className="text-muted">Aucun ban synchronisé pour l'instant.</p>}
          {jobs.map((j) => (
            <div key={j.id} className="rounded-control border border-line p-3.5">
              <div className="flex items-center gap-2.5">
                <b>
                  {j.kind === "ban" ? "Ban" : "Unban"} · {j.source_name}
                </b>
                <span className="text-muted">{new Date(j.created_at_ms).toLocaleString("fr-FR")}</span>
                {j.items.some((i) => i.status === "failed") && (
                  <Button variant="outline" className="ml-auto" disabled={busy} onClick={() => retry(j.id)}>
                    Rejouer
                  </Button>
                )}
              </div>
              <ul className="mt-2 flex flex-col gap-1.5">
                {j.items.map((i) => (
                  <li key={i.id} className="flex items-center gap-2">
                    <PlatformBadge platform={platformKey(i.platform)} size={20} />
                    <span>#{i.channel_name}</span>
                    <Badge tone={STATUS_TONE[i.status]}>{STATUS_LABEL[i.status]}</Badge>
                    {i.last_error && i.status === "failed" && <span className="truncate text-xs text-danger-text">{i.last_error}</span>}
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </Card>
      </div>

      <Card className="flex w-105 shrink-0 flex-col gap-3 self-start p-5">
        <h2 className="font-display text-base font-semibold">Table d'identités</h2>
        <p className="text-xs text-muted">Reliez les comptes d'une même personne : un ban s'applique avec l'identifiant propre à chaque plateforme.</p>
        {persons.length === 0 && <p className="text-muted">Aucune correspondance.</p>}
        {persons.map((p) => (
          <div key={p} className="rounded-control border border-line p-3">
            <b>{p}</b>
            {links
              .filter((l) => l.person === p)
              .map((l) => (
                <div key={`${l.platform}:${l.user_id}`} className="mt-1.5 flex items-center gap-2">
                  <PlatformBadge platform={platformKey(l.platform)} size={20} />
                  <span className="flex-1 truncate">
                    {l.display_name || l.user_id} <span className="text-xs text-muted">({l.user_id})</span>
                  </span>
                  <button
                    aria-label={`Retirer ${l.display_name || l.user_id}`}
                    className="px-2 text-muted hover:text-danger-text"
                    onClick={() => run(async () => { await api.bansyncUnlink(l.platform, l.user_id); refresh(); })}
                  >
                    ×
                  </button>
                </div>
              ))}
          </div>
        ))}
        <div className="grid grid-cols-2 gap-2.5 border-t border-line pt-3">
          <Field label="Personne">
            <Input list="bansync-persons" value={newLink.person} onChange={(e) => setNewLink({ ...newLink, person: e.target.value })} />
          </Field>
          <Field label="Plateforme">
            <Select value={newLink.platform} onChange={(e) => setNewLink({ ...newLink, platform: e.target.value })}>
              {PLATFORM_KEYS.map((k) => (
                <option key={k} value={k}>
                  {PLATFORMS[k].label}
                </option>
              ))}
            </Select>
          </Field>
          <Field label="Identifiant">
            <Input value={newLink.user_id} onChange={(e) => setNewLink({ ...newLink, user_id: e.target.value })} />
          </Field>
          <Field label="Pseudo">
            <Input value={newLink.display_name} onChange={(e) => setNewLink({ ...newLink, display_name: e.target.value })} />
          </Field>
          <datalist id="bansync-persons">
            {persons.map((p) => (
              <option key={p} value={p} />
            ))}
          </datalist>
          <Button className="col-span-2" disabled={busy || !newLink.person.trim() || !newLink.user_id.trim()} onClick={addLink}>
            Lier ce compte
          </Button>
        </div>
      </Card>
    </div>
  );
}
