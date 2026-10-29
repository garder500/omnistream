import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import { Badge, Button, Card, Input, Modal, PageHeader, Select } from "../components/ui";
import TrovoCard from "../components/TrovoCard";
import KickCard from "../components/KickCard";
import TiktokCard from "../components/TiktokCard";
import YoutubeCard from "../components/YoutubeCard";
import { api } from "../lib/api";
import { PlatformBadge, PLATFORMS, platformKey } from "../lib/platforms";
import { useStore } from "../lib/store";
import type { DeviceCode } from "../lib/types";

const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e));

export default function Accounts() {
  return (
    <div className="flex h-full flex-col gap-5">
      <PageHeader title="Comptes" subtitle="Connectez vos chaînes et choisissez les chats à suivre." />
      <div className="grid grid-cols-2 gap-5 xl:grid-cols-4">
        <TwitchCard />
        <TrovoCard />
        <YoutubeCard />
        <KickCard />
        <TiktokCard />
      </div>
      <Channels />
    </div>
  );
}

function TwitchCard() {
  const { accounts, notify, refreshAccounts } = useStore();
  const account = accounts.find((a) => platformKey(a.platform) === "twitch");
  const [clientId, setClientId] = useState("");
  const [savedId, setSavedId] = useState("");
  const [code, setCode] = useState<DeviceCode | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api.getSettings().then((s) => {
      setClientId(s.twitch_client_id ?? "");
      setSavedId(s.twitch_client_id ?? "");
    });
  }, []);

  // Le modal se ferme dès que le compte apparaît (événement account://changed).
  useEffect(() => {
    if (account) setCode(null);
  }, [account]);

  const login = async () => {
    setBusy(true);
    try {
      if (clientId.trim() !== savedId) {
        await api.setTwitchClientId(clientId);
        setSavedId(clientId.trim());
      }
      setCode(await api.twitchLoginStart());
    } catch (e) {
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

  return (
    <Card className="flex flex-col gap-4 p-5">
      <div className="flex items-center gap-3">
        <PlatformBadge platform="twitch" size={44} />
        <div className="min-w-0">
          <div className="text-base font-semibold">Twitch</div>
          <div className="truncate text-muted">{account ? account.display_name : "non connecté"}</div>
        </div>
      </div>
      <Badge tone={account ? "success" : "neutral"}>{account ? "Connecté" : "Non connecté"}</Badge>
      <ul className="space-y-1 leading-relaxed text-soft">
        <li>Chat en lecture et écriture</li>
        <li>Suppression, timeout et ban</li>
        <li>Spectateurs et titre du live</li>
      </ul>
      {!account && (
        <Input
          value={clientId}
          onChange={(e) => setClientId(e.target.value)}
          placeholder="Client ID de votre application Twitch"
          aria-label="Client ID Twitch"
        />
      )}
      <div className="mt-auto flex gap-2.5">
        {account ? (
          <Button variant="ghost" onClick={logout}>
            Déconnecter
          </Button>
        ) : (
          <Button variant="primary" onClick={login} disabled={busy || !clientId.trim()}>
            Se connecter
          </Button>
        )}
      </div>
      {!account && (
        <p className="text-xs leading-relaxed text-muted">
          Créez une application sur dev.twitch.tv/console avec le type de client « Public », puis collez son Client ID. Aucun secret n'est nécessaire.
        </p>
      )}

      <Modal open={code !== null} title="Autoriser Omnistream sur Twitch" onClose={() => setCode(null)}>
        {code && (
          <div className="flex flex-col gap-4">
            <p className="text-soft">Ouvrez la page d'activation de Twitch et saisissez ce code :</p>
            <div className="rounded-control bg-ink py-4 text-center font-mono text-3xl font-bold tracking-widest">{code.user_code}</div>
            <Button variant="primary" onClick={() => openUrl(code.verification_uri).catch((e) => notify(errText(e), true))}>
              Ouvrir twitch.tv/activate
            </Button>
            <p className="text-xs text-muted">En attente de votre validation… cette fenêtre se fermera toute seule.</p>
          </div>
        )}
      </Modal>
    </Card>
  );
}

function Channels() {
  const { channels, connected, addChannel, removeChannel } = useStore();
  const [name, setName] = useState("");
  const [platform, setPlatform] = useState<"twitch" | "trovo" | "youtube" | "kick" | "tiktok">("twitch");

  return (
    <Card className="p-5">
      <h2 className="mb-1 text-base font-semibold">Chaînes suivies</h2>
      <p className="mb-4 text-muted">
        Le chat de ces chaînes alimente le multi-chat. Sans compte connecté, il est en lecture seule ; avec un compte, vous pouvez écrire et modérer.
      </p>
      <form
        className="mb-4 flex gap-3"
        onSubmit={async (e) => {
          e.preventDefault();
          await addChannel(platform, name);
          setName("");
        }}
      >
        <Select value={platform} onChange={(e) => setPlatform(e.target.value as "twitch" | "trovo" | "youtube" | "kick" | "tiktok")} aria-label="Plateforme">
          <option value="twitch">Twitch</option>
          <option value="trovo">Trovo</option>
          <option value="youtube">YouTube</option>
          <option value="kick">Kick</option>
          <option value="tiktok">TikTok</option>
        </Select>
        <Input value={name} onChange={(e) => setName(e.target.value)} placeholder={platform === "youtube" ? "Handle ou identifiant de chaîne YouTube (ex. @garder500)" : `Nom de la chaîne ${PLATFORMS[platform].label} (ex. garder_live)`} aria-label={`Chaîne ${PLATFORMS[platform].label}`} className="flex-1" />
        <Button variant="primary" type="submit" disabled={!name.trim()}>
          Suivre
        </Button>
      </form>
      {channels.length === 0 && <p className="text-muted">Aucune chaîne suivie pour l'instant.</p>}
      {channels.map((c) => (
        <div key={c.id} className="flex items-center gap-3.5 border-t border-line py-3">
          <PlatformBadge platform={platformKey(c.platform)} size={30} />
          <span className="flex-1 font-semibold">{c.display_name}</span>
          <Badge tone={connected[c.id] ? "success" : "warn"}>{connected[c.id] ? "Connecté" : "Connexion…"}</Badge>
          <Button variant="ghost" onClick={() => removeChannel(c.id)}>
            Retirer
          </Button>
        </div>
      ))}
    </Card>
  );
}
