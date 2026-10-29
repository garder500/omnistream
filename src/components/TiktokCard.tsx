import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { PlatformBadge } from "../lib/platforms";
import { useStore } from "../lib/store";
import type { TiktokSettings } from "../lib/types";
import { Badge, Button, Card, Input } from "./ui";

const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e));

export default function TiktokCard() {
  const { notify } = useStore();
  const [settings, setSettings] = useState<TiktokSettings | null>(null);
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api.getTiktokSettings().then(setSettings);
  }, []);

  const save = async (value: string) => {
    setBusy(true);
    try {
      await api.setTiktokApiKey(value);
      setKey("");
      setSettings(await api.getTiktokSettings());
    } catch (e) {
      notify(errText(e), true);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card className="flex flex-col gap-4 p-5">
      <div className="flex items-center gap-3">
        <PlatformBadge platform="tiktok" size={44} />
        <div className="min-w-0">
          <div className="text-base font-semibold">TikTok</div>
          <div className="truncate text-muted">expérimental, lecture seule</div>
        </div>
      </div>
      <Badge tone={settings?.has_key ? "success" : "neutral"}>{settings?.has_key ? "Clé enregistrée" : "Clé d'API manquante"}</Badge>
      <ul className="space-y-1 leading-relaxed text-soft">
        <li>Chat, cadeaux et follows d'un pseudo en direct</li>
        <li>Sans compte TikTok : le pseudo suffit</li>
      </ul>
      <div className="flex flex-col gap-2.5">
        <Input type="password" value={key} onChange={(e) => setKey(e.target.value)} placeholder={settings?.has_key ? "Clé enregistrée (saisir pour remplacer)" : "Clé d'API Euler Stream"} aria-label="Clé d'API Euler Stream" />
      </div>
      <div className="mt-auto flex gap-2.5">
        <Button variant="primary" onClick={() => save(key)} disabled={busy || key.trim() === ""}>
          Enregistrer
        </Button>
        {settings?.has_key && (
          <Button variant="ghost" onClick={() => save("")} disabled={busy}>
            Supprimer la clé
          </Button>
        )}
      </div>
      <p className="text-xs leading-relaxed text-muted">
        TikTok n'a pas d'API de chat officielle. Omnistream passe par le service tiers Euler Stream (eulerstream.com), qui signe la connexion : il reçoit le pseudo suivi et votre clé. Ce
        n'est pas autorisé par les conditions de TikTok et peut cesser de fonctionner à tout moment (guide : docs/tiktok-setup.md).
      </p>
    </Card>
  );
}
