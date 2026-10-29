import { useState } from "react";
import { Button, Card, Input, PageHeader } from "../components/ui";
import { PlatformBadge, PLATFORMS, type PlatformKey } from "../lib/platforms";

type Status = "connected" | "expired" | "readonly";

interface AccountCard {
  platform: PlatformKey;
  handle: string;
  status: Status;
  capabilities: string[];
}

const ACCOUNTS: AccountCard[] = [
  { platform: "twitch", handle: "garder_live", status: "connected", capabilities: ["Chat en lecture et écriture", "Modération (suppression, timeout, ban)", "Titre et catégorie du live"] },
  { platform: "youtube", handle: "Garder Live", status: "connected", capabilities: ["Chat en lecture et écriture", "Modération (suppression, timeout, ban)", "Titre du live"] },
  { platform: "kick", handle: "garder", status: "expired", capabilities: ["Le jeton a expiré. Le chat Kick est en pause tant que le compte n'est pas reconnecté."] },
  { platform: "tiktok", handle: "", status: "readonly", capabilities: ["Chat et événements en lecture seule", "Pas d'OAuth : il suffit de saisir votre pseudo", "Diffusion via la clé de stream de Live Studio"] },
];

const BADGE: Record<Status, { text: string; cls: string }> = {
  connected: { text: "Connecté", cls: "bg-[#10301d] text-green-400" },
  expired: { text: "Reconnexion requise", cls: "bg-[#3a2d12] text-amber-300" },
  readonly: { text: "Lecture seule", cls: "bg-[#12303a] text-cyan-300" },
};

export default function Accounts() {
  const [tiktokHandle, setTiktokHandle] = useState("");

  return (
    <div className="flex h-full flex-col gap-5">
      <PageHeader title="Comptes" subtitle="Connectez vos chaînes. Les jetons restent chiffrés sur votre machine." />

      <div className="grid grid-cols-2 gap-5 xl:grid-cols-4">
        {ACCOUNTS.map((a) => (
          <Card key={a.platform} className="flex flex-col gap-4 p-5">
            <div className="flex items-center gap-3">
              <PlatformBadge platform={a.platform} size={44} />
              <div className="min-w-0">
                <div className="text-base font-semibold">{PLATFORMS[a.platform].label}</div>
                <div className="truncate text-muted">{a.platform === "tiktok" ? tiktokHandle || "pseudo non renseigné" : a.handle}</div>
              </div>
            </div>
            <span className={`w-fit rounded-full px-2.5 py-1 text-xs font-semibold ${BADGE[a.status].cls}`}>{BADGE[a.status].text}</span>
            <ul className="space-y-1 leading-relaxed text-[#b4b4c7]">
              {a.capabilities.map((c) => (
                <li key={c}>{c}</li>
              ))}
            </ul>
            {a.platform === "tiktok" && (
              <Input
                value={tiktokHandle}
                onChange={(e) => setTiktokHandle(e.target.value)}
                placeholder="@votre_pseudo"
                aria-label="Pseudo TikTok"
              />
            )}
            <div className="mt-auto flex gap-2.5">
              {a.status === "expired" ? (
                <Button variant="primary">Reconnecter</Button>
              ) : a.status === "readonly" ? (
                <Button variant="primary" disabled={!tiktokHandle}>
                  Suivre ce pseudo
                </Button>
              ) : (
                <Button>Permissions</Button>
              )}
              {a.status !== "readonly" && <Button variant="ghost">{a.status === "expired" ? "Retirer" : "Déconnecter"}</Button>}
            </div>
          </Card>
        ))}
      </div>

      <Card className="flex items-center gap-4 border-dashed p-5">
        <div className="flex-1">
          <div className="text-base font-semibold">Ajouter un compte</div>
          <div className="mt-1 text-muted">Une autre chaîne Twitch, un second compte YouTube, ou une plateforme compatible RTMP.</div>
        </div>
        <Button variant="primary" className="min-h-11">
          Connecter une plateforme
        </Button>
      </Card>

      <Card className="p-5">
        <div className="mb-3 font-semibold">Équipe de modération</div>
        {[
          { n: "Kaizen", where: "Twitch, Kick", role: "Modérateur" },
          { n: "Sarah.b", where: "YouTube", role: "Modératrice" },
        ].map((m, i) => (
          <div key={m.n} className={`flex items-center gap-3.5 py-2.5 ${i === 0 ? "border-b border-[#20202c]" : ""}`}>
            <span className="flex h-8 w-8 items-center justify-center rounded-full bg-[#2a2a3a] font-semibold">{m.n[0]}</span>
            <span className="flex-1">
              <b>{m.n}</b> <span className="text-muted">· {m.where}</span>
            </span>
            <span className="rounded-md bg-[#2a2a3a] px-2.5 py-0.5 text-xs">{m.role}</span>
          </div>
        ))}
      </Card>
    </div>
  );
}
