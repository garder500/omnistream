import { openUrl } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { PlatformBadge, platformKey } from "../lib/platforms";
import { useStore } from "../lib/store";
import type { TrovoSettings } from "../lib/types";
import { Badge, Button, Card, Input } from "./ui";

const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e));

export default function TrovoCard() {
  const { accounts, notify, refreshAccounts } = useStore();
  const account = accounts.find((a) => platformKey(a.platform) === "trovo");
  const [settings, setSettings] = useState<TrovoSettings | null>(null);
  const [clientId, setClientId] = useState("");
  const [secret, setSecret] = useState("");
  const [busy, setBusy] = useState(false);
  const [waiting, setWaiting] = useState(false);

  useEffect(() => {
    api.getTrovoSettings().then((s) => {
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
      await api.setTrovoApp(clientId, secret);
      setSecret("");
      setSettings(await api.getTrovoSettings());
      const url = await api.trovoLoginStart();
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
        <PlatformBadge platform="trovo" size={44} />
        <div className="min-w-0">
          <div className="text-base font-semibold">Trovo</div>
          <div className="truncate text-muted">{account ? account.display_name : "non connecté"}</div>
        </div>
      </div>
      <Badge tone={account ? "success" : waiting ? "warn" : "neutral"}>{account ? "Connecté" : waiting ? "En attente du navigateur…" : "Non connecté"}</Badge>
      <ul className="space-y-1 leading-relaxed text-soft">
        <li>Chat en lecture et écriture</li>
        <li>Suppression, timeout et ban</li>
      </ul>
      {!account && (
        <div className="flex flex-col gap-2.5">
          <Input value={clientId} onChange={(e) => setClientId(e.target.value)} placeholder="Client ID de votre application Trovo" aria-label="Client ID Trovo" />
          <Input
            type="password"
            value={secret}
            onChange={(e) => setSecret(e.target.value)}
            placeholder={settings?.has_secret ? "Secret enregistré (laisser vide)" : "Client Secret Trovo"}
            aria-label="Client Secret Trovo"
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
          Créez une application sur developer.trovo.live et déclarez-y l'adresse de redirection {settings?.redirect_uri ?? "http://127.0.0.1:41257/callback"}. Le chat se lit
          avec le Client ID seul ; la connexion (avec le secret) sert à écrire et modérer.
        </p>
      )}
    </Card>
  );
}
