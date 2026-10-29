import { useEffect, useState } from "react";
import { Badge, Button, Card, Checkbox, Field, Input, PageHeader, Select } from "../components/ui";
import { overlayApi, type OverlayInfo, type OverlaySettings, type OverlayTheme } from "../lib/overlayApi";
import { PlatformBadge, platformKey } from "../lib/platforms";
import { useStore } from "../lib/store";

const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e));

const THEMES: { id: OverlayTheme; label: string }[] = [
  { id: "orbite", label: "Orbite (sombre, violet)" },
  { id: "minimal", label: "Minimal (texte seul)" },
  { id: "clair", label: "Clair (cartes blanches)" },
];

export default function Overlays() {
  const { notify } = useStore();
  const [info, setInfo] = useState<OverlayInfo | null>(null);
  const [settings, setSettings] = useState<OverlaySettings | null>(null);

  useEffect(() => {
    overlayApi.info().then(setInfo).catch((e) => notify(errText(e), true));
    overlayApi.getSettings().then(setSettings).catch((e) => notify(errText(e), true));
  }, [notify]);

  return (
    <div className="flex h-full flex-col gap-5">
      <PageHeader title="Overlays" subtitle="Affichez le chat unifié dans OBS avec une source navigateur." />
      <div className="grid grid-cols-1 gap-5 xl:grid-cols-2">
        <ChatOverlayCard info={info} setInfo={setInfo} />
        {settings && <ThemeCard settings={settings} setSettings={setSettings} />}
      </div>
      <RecentMessages />
    </div>
  );
}

function ChatOverlayCard({ info, setInfo }: { info: OverlayInfo | null; setInfo: (i: OverlayInfo) => void }) {
  const { notify } = useStore();
  const [port, setPort] = useState("");

  useEffect(() => {
    if (info) setPort(String(info.port));
  }, [info]);

  const copy = async () => {
    if (!info) return;
    try {
      await navigator.clipboard.writeText(info.chat_url);
      notify("URL copiée : collez-la dans une source navigateur d'OBS.");
    } catch (e) {
      notify(errText(e), true);
    }
  };

  const apply = async (fn: () => Promise<OverlayInfo>) => {
    try {
      setInfo(await fn());
    } catch (e) {
      notify(errText(e), true);
      overlayApi.info().then(setInfo).catch(() => {});
    }
  };

  return (
    <Card className="flex flex-col gap-4 p-5">
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-lg font-semibold">Chat unifié</h2>
        <Badge tone={info?.running ? "success" : "neutral"}>{info?.running ? "Serveur actif" : "Serveur arrêté"}</Badge>
      </div>
      {info?.error && <p className="rounded-control border border-danger-line bg-danger-soft px-4 py-3 text-danger-text">{info.error}</p>}
      <Field label="URL de la source navigateur" hint="Uniquement accessible depuis cet ordinateur. Le jeton fait partie de l'URL : ne la partagez pas.">
        <div className="flex gap-2.5">
          <Input readOnly value={info?.chat_url ?? ""} onFocus={(e) => e.currentTarget.select()} className="font-mono text-xs" />
          <Button variant="primary" onClick={copy} disabled={!info}>
            Copier
          </Button>
        </div>
      </Field>
      <Field label="Port" hint="Entre 1024 et 65535. Le serveur redémarre lors du changement.">
        <div className="flex gap-2.5">
          <Input type="number" min={1024} max={65535} value={port} onChange={(e) => setPort(e.target.value)} className="w-40" />
          <Button variant="outline" onClick={() => apply(() => overlayApi.setPort(Number(port)))} disabled={!info || Number(port) === info.port}>
            Appliquer
          </Button>
        </div>
      </Field>
      <div className="flex flex-wrap gap-2.5">
        <Button variant="neutral" onClick={() => overlayApi.testMessage().catch((e) => notify(errText(e), true))}>
          Envoyer un message test
        </Button>
        <Button variant="outline" onClick={() => overlayApi.clear().catch((e) => notify(errText(e), true))}>
          Vider l'overlay
        </Button>
        <Button variant="ghost" onClick={() => apply(overlayApi.regenerateToken)}>
          Régénérer le jeton
        </Button>
      </div>
      <p className="text-xs leading-relaxed text-muted">
        Dans OBS : Sources, Ajouter, Navigateur, collez l'URL, largeur 500, hauteur 800. Régénérer le jeton invalide l'ancienne URL.
      </p>
    </Card>
  );
}

function ThemeCard({ settings, setSettings }: { settings: OverlaySettings; setSettings: (s: OverlaySettings) => void }) {
  const { notify } = useStore();
  const [css, setCss] = useState(settings.custom_css);

  const save = async (next: OverlaySettings) => {
    setSettings(next);
    try {
      setSettings(await overlayApi.setSettings(next));
    } catch (e) {
      notify(errText(e), true);
    }
  };

  return (
    <Card className="flex flex-col gap-4 p-5">
      <h2 className="text-lg font-semibold">Apparence</h2>
      <Field label="Thème">
        <Select value={settings.theme} onChange={(e) => save({ ...settings, theme: e.target.value as OverlayTheme })}>
          {THEMES.map((t) => (
            <option key={t.id} value={t.id}>
              {t.label}
            </option>
          ))}
        </Select>
      </Field>
      <div className="grid grid-cols-2 gap-4">
        <NumberField label="Taille du texte (px)" min={10} max={96} value={settings.font_size} onCommit={(n) => save({ ...settings, font_size: n })} />
        <NumberField label="Messages affichés" min={1} max={100} value={settings.max_messages} onCommit={(n) => save({ ...settings, max_messages: n })} />
      </div>
      <Checkbox checked={settings.show_platform} onChange={(v) => save({ ...settings, show_platform: v })}>
        Afficher la pastille de plateforme
      </Checkbox>
      <Field label="CSS personnalisé" hint="Ajouté après le thème. Classes : .msg, .author, .text, .badge, .hl (surligné), .pin (épinglé).">
        <textarea
          value={css}
          onChange={(e) => setCss(e.target.value)}
          rows={5}
          spellCheck={false}
          className="w-full rounded-control border border-line-strong bg-field px-4 py-2.5 font-mono text-xs text-zinc-100 outline-none focus:border-brand focus:ring-4 focus:ring-brand/20"
        />
      </Field>
      <div>
        <Button variant="outline" onClick={() => save({ ...settings, custom_css: css })} disabled={css === settings.custom_css}>
          Appliquer le CSS
        </Button>
      </div>
    </Card>
  );
}

/** Champ numérique à brouillon local : la valeur n'est envoyée (et bornée) qu'à la fin de la saisie. */
function NumberField({ label, value, min, max, onCommit }: { label: string; value: number; min: number; max: number; onCommit: (n: number) => void }) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  const commit = () => {
    const n = Math.min(max, Math.max(min, Math.round(Number(draft))));
    if (!Number.isFinite(n) || draft.trim() === "") return setDraft(String(value));
    setDraft(String(n));
    if (n !== value) onCommit(n);
  };
  return (
    <Field label={label}>
      <Input
        type="number"
        min={min}
        max={max}
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
      />
    </Field>
  );
}

/** Derniers messages du chat, avec épinglage et surlignage vers l'overlay. */
function RecentMessages() {
  const { messages, notify } = useStore();
  const [pinnedId, setPinnedId] = useState<string | null>(null);
  const [lit, setLit] = useState<Record<string, boolean>>({});
  const recent = messages.filter((m) => !m.deleted).slice(-12).reverse();
  const fail = (e: unknown) => notify(errText(e), true);

  return (
    <Card className="p-5">
      <h2 className="mb-3 text-lg font-semibold">Mettre en avant un message</h2>
      {recent.length === 0 ? (
        <p className="text-muted">Les messages du chat apparaîtront ici. Vous pourrez les épingler ou les surligner dans l'overlay.</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {recent.map((m) => (
            <li key={m.id} className="flex items-center gap-3 rounded-control border border-line px-3 py-2">
              <PlatformBadge platform={platformKey(m.platform)} size={24} />
              <span className="font-semibold">{m.author.display_name}</span>
              <span className="min-w-0 flex-1 truncate text-soft">{m.text}</span>
              <Button
                variant="outline"
                className="!min-h-9 !px-3"
                onClick={() => {
                  const on = !lit[m.id];
                  setLit((l) => ({ ...l, [m.id]: on }));
                  overlayApi.highlight(m.id, on).catch(fail);
                }}
              >
                {lit[m.id] ? "Retirer le surlignage" : "Surligner"}
              </Button>
              <Button
                variant={pinnedId === m.id ? "warn" : "outline"}
                className="!min-h-9 !px-3"
                onClick={() => {
                  if (pinnedId === m.id) {
                    setPinnedId(null);
                    overlayApi.unpin().catch(fail);
                  } else {
                    setPinnedId(m.id);
                    overlayApi.pin(m).catch(fail);
                  }
                }}
              >
                {pinnedId === m.id ? "Désépingler" : "Épingler"}
              </Button>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}
