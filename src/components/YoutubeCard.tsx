import { openUrl } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { PlatformBadge, platformKey } from "../lib/platforms";
import { useStore } from "../lib/store";
import type { YoutubeSettings } from "../lib/types";
import { Badge, Button, Card, Input } from "./ui";

const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e));

export default function YoutubeCard() {
  const { accounts, notify, refreshAccounts } = useStore();
  const account = accounts.find((a) => platformKey(a.platform) === "youtube");
  const [settings, setSettings] = useState<YoutubeSettings | null>(null);
  const [clientId, setClientId] = useState("");
  const [secret, setSecret] = useState("");
  const [busy, setBusy] = useState(false);
  const [waiting, setWaiting] = useState(false);

  useEffect(() => {
    api.getYoutubeSettings().then((s) => {
      setSettings(s);
      setClientId(s.client_id ?? "");
    });
  }, []);

  // Une erreur de connexion (refus, délai dépassé...) ramène la carte à l'état initial.
  useEffect(() => {
    const unlisten = listen("account://error", () => setWaiting(false));
    return () => {
      unlisten.then((f) => f()).catch(() => {});
    };
  }, []);

  useEffect(() => {
    if (account) setWaiting(false);
  }, [account]);

  const login = async () => {
    setBusy(true);
    try {
      await api.setYoutubeApp(clientId, secret);
      setSecret("");
      setSettings(await api.getYoutubeSettings());
      const url = await api.youtubeLoginStart();
      setWaiting(true);
      await openUrl(url);
    } catch (e) {
      setWaiting(false);
      notify(errText(e), true);
    } finally {
      setBusy(false);
    }
  };

  const logout = async () => {
    if (!account) return;
    try {
      await api.removeAccount(account.id);
      await refreshAccounts();
    } catch (e) {
      notify(errText(e), true);
    }
  };

  const ready = clientId.trim() !== "" && (secret.trim() !== "" || settings?.has_secret);

  return (
    <Card className="flex flex-col gap-4 p-5">
      <div className="flex items-center gap-3">
        <PlatformBadge platform="youtube" size={44} />
        <div className="min-w-0">
          <div className="text-base font-semibold">YouTube</div>
          <div className="truncate text-muted">{account ? account.display_name : "non connecté"}</div>
        </div>
      </div>
      <Badge tone={account ? "success" : waiting ? "warn" : "neutral"}>{account ? "Connecté" : waiting ? "En attente du navigateur…" : "Non connecté"}</Badge>
      <ul className="space-y-1 leading-relaxed text-soft">
        <li>Chat du direct en lecture et écriture</li>
        <li>Suppression, timeout et ban</li>
      </ul>
      {!account && (
        <div className="flex flex-col gap-2.5">
          <Input value={clientId} onChange={(e) => setClientId(e.target.value)} placeholder="Client ID OAuth Google (application de bureau)" aria-label="Client ID YouTube" />
          <Input
            type="password"
            value={secret}
            onChange={(e) => setSecret(e.target.value)}
            placeholder={settings?.has_secret ? "Secret enregistré (laisser vide)" : "Code secret du client Google"}
            aria-label="Code secret du client Google"
          />
        </div>
      )}
      <div className="mt-auto flex gap-2.5">
        {account ? (
          <Button variant="ghost" onClick={logout}>
            Déconnecter
          </Button>
        ) : (
          <Button variant="primary" onClick={login} disabled={busy || !ready}>
            Se connecter
          </Button>
        )}
      </div>
      {!account && (
        <p className="text-xs leading-relaxed text-muted">
          Créez des identifiants OAuth de type « Application de bureau » dans Google Cloud, avec l'API YouTube Data v3 activée (guide : docs/youtube-setup.md). Redirection
          utilisée : {settings?.redirect_uri ?? "http://127.0.0.1:41258/callback"}. Quota Google de 10 000 unités par jour : le chat d'un direct en consomme environ 3 000 par heure.
        </p>
      )}
    </Card>
  );
}
