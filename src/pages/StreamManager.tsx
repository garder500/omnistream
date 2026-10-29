import { useState } from "react";
import { Button, Card, PageHeader, Toggle } from "../components/ui";
import { PlatformBadge, PLATFORMS, type PlatformKey } from "../lib/platforms";

interface Destination {
  platform: PlatformKey;
  server: string;
  key: string;
  enabled: boolean;
}

const INITIAL: Destination[] = [
  { platform: "twitch", server: "rtmp://live.twitch.tv/app", key: "live_••••••••••", enabled: true },
  { platform: "youtube", server: "rtmp://a.rtmp.youtube.com/live2", key: "••••-••••-••••", enabled: true },
  { platform: "kick", server: "rtmps://…global-contribute.live-video.net", key: "sk_••••••••••", enabled: true },
  { platform: "tiktok", server: "Serveur indiqué dans TikTok Live Studio", key: "", enabled: false },
];

export default function StreamManager() {
  const [title, setTitle] = useState("Reprise du speedrun · Partie 2");
  const [destinations, setDestinations] = useState(INITIAL);
  const active = destinations.filter((d) => d.enabled).length;

  const update = (platform: PlatformKey, patch: Partial<Destination>) =>
    setDestinations((ds) => ds.map((d) => (d.platform === platform ? { ...d, ...patch } : d)));

  return (
    <div className="flex h-full flex-col gap-5">
      <PageHeader
        title="Stream manager"
        subtitle="Un flux entrant, plusieurs destinations"
        actions={
          <Button variant="primary" className="min-h-11" disabled={active === 0}>
            Passer en direct sur {active} plateforme{active > 1 ? "s" : ""}
          </Button>
        }
      />

      <Card className="flex items-center gap-5 px-5 py-4.5">
        <div className="flex h-11 w-11 items-center justify-center rounded-xl bg-[#1e1b3a]">
          <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="#a78bfa" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
            <path d="M3 12h6l3-8 3 16 3-8h3" />
          </svg>
        </div>
        <div className="flex-1">
          <div className="font-semibold">Entrée OBS</div>
          <div className="mt-0.5 font-mono text-[13px] text-muted">rtmp://127.0.0.1:1935/live · clé : ••••••••</div>
        </div>
        <span className="flex items-center gap-2 font-semibold text-green-400">
          <span className="h-2 w-2 rounded-full bg-green-400" />
          Signal reçu · 1080p60 · 6,1 Mb/s
        </span>
      </Card>

      <Card className="overflow-hidden">
        <div className="grid grid-cols-[180px_1fr_1fr_160px_70px] gap-4 border-b border-[#20202c] px-5 py-3.5 text-[13px] text-muted">
          <span>Plateforme</span>
          <span>Titre du live</span>
          <span>Serveur RTMP</span>
          <span>Clé de stream</span>
          <span className="text-right">Actif</span>
        </div>
        {destinations.map((d, i) => (
          <div
            key={d.platform}
            className={`grid grid-cols-[180px_1fr_1fr_160px_70px] items-center gap-4 px-5 py-4 ${i < destinations.length - 1 ? "border-b border-[#20202c]" : ""}`}
          >
            <span className="flex items-center gap-2.5 font-semibold">
              <PlatformBadge platform={d.platform} size={28} />
              {PLATFORMS[d.platform].label}
            </span>
            {PLATFORMS[d.platform].readOnly ? (
              <span className="text-[13px] text-muted">Le titre se règle dans TikTok Live Studio.</span>
            ) : (
              <input
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                aria-label={`Titre ${PLATFORMS[d.platform].label}`}
                className="rounded-lg border border-[#2a2a3a] bg-ink px-3 py-2.5 outline-none focus:border-brand"
              />
            )}
            <span className="truncate font-mono text-[13px] text-[#b4b4c7]">{d.server}</span>
            {d.platform === "tiktok" ? (
              <input
                placeholder="Coller la clé"
                aria-label="Clé de stream TikTok"
                className="rounded-lg border border-[#2a2a3a] bg-ink px-3 py-2.5 font-mono text-[13px] outline-none focus:border-brand"
                onChange={(e) => update("tiktok", { key: e.target.value })}
              />
            ) : (
              <span className="font-mono text-[#b4b4c7]">{d.key}</span>
            )}
            <span className="flex justify-end">
              <Toggle checked={d.enabled} onChange={(v) => update(d.platform, { enabled: v })} label={`Diffuser sur ${PLATFORMS[d.platform].label}`} />
            </span>
          </div>
        ))}
      </Card>

      <div className="flex gap-4">
        <Button className="border border-dashed border-[#3a3a50] !bg-panel">+ Ajouter une destination (RTMP personnalisé)</Button>
        <Button className="border border-[#2a2a3a] !bg-panel">Appliquer le titre à toutes</Button>
      </div>
      <p className="text-xs text-muted">
        TikTok exige un compte éligible : la clé se copie depuis TikTok Live Studio (aucun OAuth disponible).
      </p>
    </div>
  );
}
