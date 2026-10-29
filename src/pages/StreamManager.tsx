import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { Badge, Button, Card, Field, Input, Modal, PageHeader, Select, Toggle } from "../components/ui";
import { api } from "../lib/api";
import { PlatformBadge, PLATFORMS, platformKey, type PlatformKey } from "../lib/platforms";
import { useStore } from "../lib/store";
import type {
  DestinationInput,
  DestinationStatus,
  DestState,
  IngestStatus,
  Platform,
  StreamDestination,
  StreamingEvent,
  StreamingState,
} from "../lib/types";

const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e));

type Kind = PlatformKey | "custom";

const PRESETS: Record<Kind, { label: string; url: string; hint?: string }> = {
  twitch: { label: "Twitch", url: "rtmp://live.twitch.tv/app" },
  youtube: { label: "YouTube", url: "rtmp://a.rtmp.youtube.com/live2" },
  kick: { label: "Kick", url: "", hint: "Serveur et clé sont dans le tableau de bord Kick (Paramètres du stream)." },
  tiktok: { label: "TikTok", url: "", hint: "TikTok exige un compte éligible : serveur et clé se copient depuis TikTok Live Studio." },
  custom: { label: "RTMP personnalisé", url: "" },
};

const kindOf = (p: Platform): Kind => {
  const key = typeof p === "string" ? p : p.other;
  return key in PLATFORMS ? (key as PlatformKey) : "custom";
};
const platformOf = (k: Kind): Platform => (k === "custom" ? { other: "rtmp" } : k);

const STATE_LABEL: Record<DestState, { text: string; tone: "neutral" | "info" | "success" | "warn" | "danger" }> = {
  idle: { text: "Prêt", tone: "neutral" },
  waiting_signal: { text: "En attente du signal", tone: "info" },
  connecting: { text: "Connexion…", tone: "info" },
  live: { text: "En direct", tone: "success" },
  retrying: { text: "Nouvelle tentative", tone: "warn" },
  error: { text: "Erreur", tone: "danger" },
};

export default function StreamManager() {
  const { notify } = useStore();
  const [state, setState] = useState<StreamingState | null>(null);
  const [destinations, setDestinations] = useState<StreamDestination[]>([]);
  const [editing, setEditing] = useState<DestinationInput | null>(null);
  const [busy, setBusy] = useState(false);

  const fail = useCallback((e: unknown) => notify(errText(e), true), [notify]);

  const reload = useCallback(async () => {
    const [s, d] = await Promise.all([api.streamingState(), api.listDestinations()]);
    setState(s);
    setDestinations(d);
  }, []);

  useEffect(() => {
    reload().catch(fail);
  }, [reload, fail]);

  // État poussé par le backend : signal entrant, statut de chaque destination, direct on/off.
  useEffect(() => {
    let cancelled = false;
    const un = listen<StreamingEvent>("stream://event", ({ payload }) => {
      if (cancelled) return;
      if (payload.type === "ingest") {
        setState((s) => s && { ...s, ingest: { ...s.ingest, status: payload.data } });
      } else if (payload.type === "live") {
        setState((s) => s && { ...s, live: payload.data });
      } else {
        const status: DestinationStatus = payload.data;
        setDestinations((ds) => ds.map((d) => (d.id === status.id ? { ...d, status } : d)));
      }
    });
    return () => {
      cancelled = true;
      un.then((fn) => fn());
    };
  }, []);

  const enabled = destinations.filter((d) => d.enabled);
  const live = state?.live ?? false;
  const ffmpegMissing = state !== null && !state.ffmpeg.found;

  const toggleLive = async () => {
    setBusy(true);
    try {
      if (live) await api.stopLive();
      else await api.goLive();
      await reload();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };

  const setEnabled = async (d: StreamDestination, value: boolean) => {
    try {
      await api.saveDestination({ id: d.id, platform: d.platform, name: d.name, rtmp_url: d.rtmp_url, enabled: value });
      setDestinations(await api.listDestinations());
    } catch (e) {
      fail(e);
    }
  };

  return (
    <div className="flex h-full flex-col gap-5 overflow-y-auto pr-1">
      <PageHeader
        title="Stream manager"
        subtitle="Un flux entrant, plusieurs destinations"
        actions={
          <Button variant={live ? "danger" : "primary"} disabled={busy || ffmpegMissing || (!live && enabled.length === 0)} onClick={toggleLive}>
            {live ? "Arrêter le direct" : `Passer en direct sur ${enabled.length} plateforme${enabled.length > 1 ? "s" : ""}`}
          </Button>
        }
      />

      {ffmpegMissing && state && <FfmpegCard state={state} onChange={reload} />}
      {state && <IngestCard state={state} onPortChanged={reload} />}
      <TitleCard initial={state?.title ?? ""} />

      <Card className="overflow-hidden">
        {destinations.length === 0 ? (
          <div className="px-5 py-10 text-center text-muted">
            Aucune destination pour l'instant. Ajoutez Twitch, YouTube, Kick, TikTok ou un serveur RTMP personnalisé.
          </div>
        ) : (
          destinations.map((d, i) => (
            <DestinationRow
              key={d.id}
              d={d}
              last={i === destinations.length - 1}
              onToggle={(v) => setEnabled(d, v)}
              onEdit={() => setEditing({ id: d.id, platform: d.platform, name: d.name, rtmp_url: d.rtmp_url, enabled: d.enabled })}
            />
          ))
        )}
      </Card>

      <div className="flex gap-4">
        <Button
          variant="dashed"
          onClick={() => setEditing({ platform: "twitch", name: "Twitch", rtmp_url: PRESETS.twitch.url, enabled: true })}
        >
          + Ajouter une destination
        </Button>
      </div>

      <DestinationModal
        value={editing}
        hasKey={destinations.find((d) => d.id === editing?.id)?.has_key ?? false}
        onClose={() => setEditing(null)}
        onSave={async (input) => {
          try {
            await api.saveDestination(input);
            setEditing(null);
            setDestinations(await api.listDestinations());
          } catch (e) {
            fail(e);
          }
        }}
        onDelete={async (id) => {
          try {
            await api.deleteDestination(id);
            setEditing(null);
            setDestinations(await api.listDestinations());
          } catch (e) {
            fail(e);
          }
        }}
      />
    </div>
  );
}

function FfmpegCard({ state, onChange }: { state: StreamingState; onChange: () => Promise<void> }) {
  const { notify } = useStore();
  const [path, setPath] = useState("");
  return (
    <Card className="flex flex-col gap-3 border-warn px-5 py-4.5">
      <div className="font-semibold text-warn">FFmpeg est introuvable</div>
      <p className="text-sm text-soft">{state.ffmpeg.message}</p>
      <div className="flex gap-3">
        <Input value={path} onChange={(e) => setPath(e.target.value)} placeholder="Chemin de ffmpeg (ex. C:\ffmpeg\bin\ffmpeg.exe)" aria-label="Chemin de FFmpeg" />
        <Button
          variant="outline"
          onClick={async () => {
            try {
              const s = await api.setFfmpegPath(path);
              notify(s.found ? "FFmpeg détecté" : "FFmpeg toujours introuvable à cet emplacement", !s.found);
              await onChange();
            } catch (e) {
              notify(errText(e), true);
            }
          }}
        >
          Vérifier
        </Button>
      </div>
    </Card>
  );
}

function describeSignal(s: IngestStatus): string {
  const res = s.height ? `${s.height}p${s.fps ? Math.round(s.fps) : ""}` : null;
  const rate = s.bitrate_kbps != null ? `${(s.bitrate_kbps / 1000).toFixed(1).replace(".", ",")} Mb/s` : null;
  return ["Signal reçu", res, s.video_codec, rate].filter(Boolean).join(" · ");
}

function IngestCard({ state, onPortChanged }: { state: StreamingState; onPortChanged: () => Promise<void> }) {
  const { notify } = useStore();
  const { ingest } = state;
  const [reveal, setReveal] = useState(false);
  const [port, setPort] = useState(String(ingest.port));
  const status = ingest.status;

  const copy = (text: string, what: string) =>
    navigator.clipboard.writeText(text).then(
      () => notify(`${what} copié`),
      () => notify("Copie impossible", true),
    );

  return (
    <Card className="flex flex-col gap-4 px-5 py-4.5">
      <div className="flex items-center gap-5">
        <div className="flex h-11 w-11 items-center justify-center rounded-control bg-brand-soft">
          <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="#a78bfa" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
            <path d="M3 12h6l3-8 3 16 3-8h3" />
          </svg>
        </div>
        <div className="min-w-0 flex-1">
          <div className="font-semibold">Entrée OBS</div>
          <div className="mt-0.5 text-[13px] text-muted">Dans OBS : Paramètres → Stream → Service « Personnalisé ».</div>
        </div>
        {status.state === "live" && (
          <span className="flex items-center gap-2 font-semibold text-success">
            <span className="h-2 w-2 rounded-full bg-green-400" />
            {describeSignal(status)}
          </span>
        )}
        {status.state === "waiting" && <span className="text-muted">En attente d'OBS…</span>}
        {status.state === "error" && <span className="font-semibold text-danger-text">Serveur d'entrée arrêté</span>}
      </div>

      <div className="grid grid-cols-2 gap-4">
        <Field label="Serveur">
          <div className="flex gap-2">
            <Input readOnly value={ingest.url} className="font-mono text-[13px]" />
            <Button variant="outline" onClick={() => copy(ingest.url, "Serveur")}>Copier</Button>
          </div>
        </Field>
        <Field label="Clé de stream">
          <div className="flex gap-2">
            <Input readOnly type={reveal ? "text" : "password"} value={ingest.key} className="font-mono text-[13px]" />
            <Button variant="outline" onClick={() => setReveal((r) => !r)}>{reveal ? "Masquer" : "Afficher"}</Button>
            <Button variant="outline" onClick={() => copy(ingest.key, "Clé")}>Copier</Button>
          </div>
        </Field>
      </div>

      {status.state === "error" && (
        <div className="flex flex-col gap-3 rounded-control border border-danger-line bg-danger-soft p-4">
          <p className="text-sm text-danger-text">{status.error}</p>
          <div className="flex items-end gap-3">
            <Field label="Port d'entrée">
              <Input value={port} onChange={(e) => setPort(e.target.value)} inputMode="numeric" className="w-32" />
            </Field>
            <Button
              variant="outline"
              onClick={async () => {
                try {
                  await api.setIngestPort(Number(port));
                  await onPortChanged();
                } catch (e) {
                  notify(errText(e), true);
                }
              }}
            >
              Réessayer
            </Button>
          </div>
        </div>
      )}
    </Card>
  );
}

function TitleCard({ initial }: { initial: string }) {
  const { notify, accounts } = useStore();
  const [title, setTitle] = useState(initial);
  const hasTwitch = accounts.some((a) => platformKey(a.platform) === "twitch");

  useEffect(() => setTitle(initial), [initial]);
  // Préremplit avec le titre actuel de la chaîne Twitch quand aucun titre n'est mémorisé.
  useEffect(() => {
    if (initial || !hasTwitch) return;
    api.twitchCurrentTitle().then((t) => t && setTitle(t)).catch(() => {});
  }, [initial, hasTwitch]);

  const apply = async () => {
    try {
      const results = await api.setStreamTitle(title);
      if (results.length === 0) notify("Titre enregistré. Connectez Twitch (page Comptes) pour l'appliquer automatiquement.");
      for (const r of results) notify(`${PRESETS[r.platform as Kind]?.label ?? r.platform} : ${r.message}`, !r.ok);
    } catch (e) {
      notify(errText(e), true);
    }
  };

  return (
    <Card className="flex items-end gap-4 px-5 py-4.5">
      <div className="flex-1">
        <Field label="Titre du live" hint="Mis à jour automatiquement sur Twitch (si connecté). YouTube, Kick et TikTok se règlent pour l'instant sur leur plateforme.">
          <Input value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Titre de votre live" />
        </Field>
      </div>
      <Button variant="outline" onClick={apply}>Appliquer le titre</Button>
    </Card>
  );
}

function DestinationRow({ d, last, onToggle, onEdit }: { d: StreamDestination; last: boolean; onToggle: (v: boolean) => void; onEdit: () => void }) {
  const kind = kindOf(d.platform);
  const st = STATE_LABEL[d.status.state];
  const label = d.status.state === "live" && d.status.bitrate_kbps ? `En direct · ${Math.round(d.status.bitrate_kbps)} kb/s` : st.text;
  return (
    <div className={`flex items-center gap-4 px-5 py-4 ${last ? "" : "border-b border-line"}`}>
      <span className="flex w-52 shrink-0 items-center gap-2.5 font-semibold">
        {kind === "custom" ? (
          <span className="inline-flex h-7 w-7 items-center justify-center rounded-md bg-line-strong text-sm font-bold">R</span>
        ) : (
          <PlatformBadge platform={kind} size={28} />
        )}
        <span className="truncate">{d.name}</span>
      </span>
      <div className="min-w-0 flex-1">
        <div className="truncate font-mono text-[13px] text-soft">{d.rtmp_url}</div>
        <div className="mt-0.5 text-xs text-muted">{d.has_key ? "Clé enregistrée de façon sécurisée" : "Clé manquante"}</div>
        {d.status.error && d.status.state !== "live" && <div className="mt-1 text-xs text-danger-text">{d.status.error}</div>}
      </div>
      {d.enabled && d.status.state !== "idle" && <Badge tone={st.tone}>{label}</Badge>}
      <Button variant="outline" onClick={onEdit}>Modifier</Button>
      <Toggle checked={d.enabled} onChange={onToggle} label={`Diffuser sur ${d.name}`} />
    </div>
  );
}

function DestinationModal({
  value,
  hasKey,
  onClose,
  onSave,
  onDelete,
}: {
  value: DestinationInput | null;
  hasKey: boolean;
  onClose: () => void;
  onSave: (input: DestinationInput) => void;
  onDelete: (id: string) => void;
}) {
  const [draft, setDraft] = useState<DestinationInput | null>(value);
  const [key, setKey] = useState("");
  useEffect(() => {
    setDraft(value);
    setKey("");
  }, [value]);
  if (!draft) return <Modal open={false} title="" onClose={onClose}>{null}</Modal>;

  const kind = kindOf(draft.platform);
  const preset = PRESETS[kind];
  const patch = (p: Partial<DestinationInput>) => setDraft({ ...draft, ...p });

  return (
    <Modal open title={draft.id ? "Modifier la destination" : "Nouvelle destination"} onClose={onClose}>
      <div className="flex flex-col gap-4">
        <Field label="Plateforme">
          <Select
            value={kind}
            onChange={(e) => {
              const k = e.target.value as Kind;
              patch({ platform: platformOf(k), name: PRESETS[k].label, rtmp_url: PRESETS[k].url });
            }}
          >
            {(Object.keys(PRESETS) as Kind[]).map((k) => (
              <option key={k} value={k}>{PRESETS[k].label}</option>
            ))}
          </Select>
        </Field>
        <Field label="Nom">
          <Input value={draft.name} onChange={(e) => patch({ name: e.target.value })} />
        </Field>
        <Field label="Serveur RTMP" hint={preset.hint}>
          <Input value={draft.rtmp_url} onChange={(e) => patch({ rtmp_url: e.target.value })} placeholder="rtmp://… ou rtmps://…" className="font-mono text-[13px]" />
        </Field>
        <Field label="Clé de stream" hint={hasKey ? "Une clé est enregistrée : laissez vide pour la conserver." : undefined}>
          <Input type="password" value={key} onChange={(e) => setKey(e.target.value)} placeholder={hasKey ? "••••••••••" : "Coller la clé"} className="font-mono text-[13px]" autoComplete="off" />
        </Field>
        <div className="mt-2 flex justify-between gap-3">
          {draft.id ? (
            <Button variant="ghost" onClick={() => onDelete(draft.id!)}>Supprimer</Button>
          ) : (
            <span />
          )}
          <div className="flex gap-3">
            <Button variant="outline" onClick={onClose}>Annuler</Button>
            <Button variant="primary" onClick={() => onSave({ ...draft, stream_key: key.trim() || undefined })}>Enregistrer</Button>
          </div>
        </div>
      </div>
    </Modal>
  );
}
