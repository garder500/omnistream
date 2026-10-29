import { useState } from "react";
import { useAi, type AiSuggestion } from "../lib/ai";
import { useStore } from "../lib/store";
import { Badge, Button, Card, CardHeader, Checkbox, Field, Input, LinkButton } from "./ui";

const pct = (n: number) => `${Math.round(n * 100)} %`;

/** Carte de consentement + réglages de l'assistant IA (clé API Claude de l'utilisateur). */
export function AiSettingsCard({ onDone }: { onDone?: () => void }) {
  const { settings, save } = useAi();
  const [consent, setConsent] = useState(false);
  const [key, setKey] = useState("");
  const [modModel, setModModel] = useState(settings?.moderation_model ?? "claude-haiku-5-5");
  const [sumModel, setSumModel] = useState(settings?.summary_model ?? "claude-sonnet-5-5");
  const active = !!settings?.enabled;
  const hasKey = !!settings?.has_key || key.trim() !== "";

  /** `newKey` : undefined = ne pas toucher à la clé, "" = la supprimer. */
  const apply = async (enabled: boolean, newKey: string | undefined) => {
    const ok = await save({ enabled, consent: enabled && (consent || active), api_key: newKey, moderation_model: modModel, summary_model: sumModel });
    if (ok) {
      setKey("");
      setConsent(false);
      onDone?.();
    }
  };

  return (
    <Card className="p-5">
      <div className="mb-3 flex items-center gap-3">
        <h2 className="font-display text-base font-semibold">Assistant IA (clé Claude personnelle)</h2>
        <Badge tone={active ? "success" : "neutral"}>{active ? "Activé" : "Désactivé"}</Badge>
      </div>

      <div role="note" className="mb-4 rounded-control border border-warn/40 bg-warn-soft p-4 leading-relaxed text-warn">
        <b>Confidentialité.</b> Une fois activé, le pseudo et le texte des messages de votre chat sont envoyés à l'API d'Anthropic (Claude) avec{" "}
        <b>votre</b> clé. Vos spectateurs n'en sont pas informés par Omnistream, et les appels sont facturés sur votre compte Anthropic. La clé reste sur cet ordinateur
        et n'est jamais affichée ni journalisée. L'IA ne fait que <b>suggérer</b> : aucune sanction n'est appliquée sans votre clic.
      </div>

      <div className="grid grid-cols-3 gap-4">
        <Field label="Clé API Claude" hint={settings?.has_key ? "Une clé est enregistrée (laisser vide pour la conserver)." : "Commence par sk-ant-… (console.anthropic.com)"}>
          <Input type="password" autoComplete="off" value={key} onChange={(e) => setKey(e.target.value)} placeholder={settings?.has_key ? "••••••••••••" : "sk-ant-…"} />
        </Field>
        <Field label="Modèle de modération" hint="Rapide et économique">
          <Input value={modModel} onChange={(e) => setModModel(e.target.value)} />
        </Field>
        <Field label="Modèle des résumés" hint="Plus qualitatif">
          <Input value={sumModel} onChange={(e) => setSumModel(e.target.value)} />
        </Field>
      </div>

      {!active && (
        <div className="mt-3">
          <Checkbox checked={consent} onChange={setConsent}>
            J'ai compris que les messages de mon chat seront envoyés à Anthropic avec ma clé.
          </Checkbox>
        </div>
      )}

      <div className="mt-4 flex flex-wrap gap-3">
        {active ? (
          <>
            <Button variant="primary" onClick={() => apply(true, key.trim() || undefined)}>
              Enregistrer
            </Button>
            <Button variant="outline" onClick={() => apply(false, undefined)}>
              Désactiver l'IA
            </Button>
          </>
        ) : (
          <Button variant="primary" disabled={!consent || !hasKey} onClick={() => apply(true, key.trim() || undefined)}>
            Activer l'assistant IA
          </Button>
        )}
        {settings?.has_key && (
          <Button variant="ghost" onClick={() => apply(false, "")}>
            Supprimer la clé
          </Button>
        )}
      </div>
    </Card>
  );
}

function SuggestionRow({ s }: { s: AiSuggestion }) {
  const { messages, moderate } = useStore();
  const { dropSuggestion } = useAi();
  const m = messages.find((x) => x.id === s.messageId);
  if (!m || !s.verdict.suggestion) return null;
  const v = s.verdict;
  const ban = v.suggestion === "ban";
  const mins = Math.round(v.timeout_seconds / 60);
  const label = ban ? "Banni (suggestion IA)" : `Timeout ${mins} min (suggestion IA)`;

  const accept = async () => {
    const kind = ban
      ? ({ kind: "ban", user_id: m.author.external_id, reason: v.reason || undefined } as const)
      : ({ kind: "timeout", user_id: m.author.external_id, seconds: v.timeout_seconds } as const);
    if (await moderate(m, kind, label)) dropSuggestion(s.messageId);
  };

  return (
    <div className="flex items-center gap-4 rounded-control border border-line bg-panel p-3.5">
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <b>{m.author.display_name}</b>
          <Badge tone={ban ? "danger" : "warn"}>{ban ? "Bannissement suggéré" : `Timeout ${mins} min suggéré`}</Badge>
          <span className="text-xs text-muted">
            spam {pct(v.spam)} · haine {pct(v.hate)} · arnaque {pct(v.scam)}
          </span>
        </div>
        <div className="mt-1 truncate text-zinc-300">{m.text}</div>
        {v.reason && <div className="mt-0.5 text-xs text-muted">IA : {v.reason}</div>}
      </div>
      <Button variant={ban ? "danger" : "warn"} onClick={accept}>
        {ban ? "Bannir" : `Timeout ${mins} min`}
      </Button>
      <Button variant="outline" onClick={() => dropSuggestion(s.messageId)}>
        Ignorer
      </Button>
    </div>
  );
}

/** Zone IA de la page Modération : réglages si inactif, sinon file de suggestions à valider. */
export function AiModerationPanel() {
  const { settings, suggestions, pending } = useAi();
  const [editing, setEditing] = useState(false);
  const enabled = !!settings?.enabled && settings.has_key;

  if (!settings) return null;
  if (!enabled || editing) return <div className="mb-5"><AiSettingsCard onDone={() => setEditing(false)} /></div>;

  return (
    <Card className="mb-5 overflow-hidden">
      <CardHeader
        title="Suggestions de l'assistant IA"
        aside={
          <span className="flex items-center gap-3">
            {pending > 0 ? `Analyse de ${pending} message${pending > 1 ? "s" : ""}…` : "Rien à valider = rien de suspect"}
            <LinkButton onClick={() => setEditing(true)}>Réglages</LinkButton>
          </span>
        }
      />
      <div className="flex max-h-64 flex-col gap-2 overflow-auto p-3">
        {suggestions.length === 0 && <p className="p-3 text-center text-muted">Aucune suggestion pour l'instant. Rien n'est appliqué sans votre validation.</p>}
        {suggestions.map((s) => (
          <SuggestionRow key={s.messageId} s={s} />
        ))}
      </div>
    </Card>
  );
}
