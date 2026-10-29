import { useAi } from "../lib/ai";
import { Button, Card, CardHeader, LinkButton } from "./ui";

/** Résumé du chat pour les arrivants tardifs + bilan de fin de live (Dashboard). */
export function AiSummaryCard({ goModeration }: { goModeration: () => void }) {
  const { settings, summary, summarizing, summaryError, summarize } = useAi();
  const enabled = !!settings?.enabled && settings.has_key;

  return (
    <Card className="flex flex-col overflow-hidden">
      <CardHeader title="Résumé IA du chat" aside={!enabled && <LinkButton onClick={goModeration}>Activer →</LinkButton>} />
      <div className="flex flex-col gap-3 p-5">
        {!enabled ? (
          <p className="text-muted">Désactivé. Activez l'assistant IA (avec votre propre clé Claude) pour résumer le chat en direct et obtenir un bilan de fin de live.</p>
        ) : (
          <>
            <div className="flex flex-wrap gap-3">
              <Button variant="primary" disabled={summarizing !== null} onClick={() => summarize("live")}>
                {summarizing === "live" ? "Résumé en cours…" : "Résumer pour les arrivants"}
              </Button>
              <Button variant="outline" disabled={summarizing !== null} onClick={() => summarize("recap")}>
                {summarizing === "recap" ? "Bilan en cours…" : "Bilan de fin de live"}
              </Button>
            </div>
            {summaryError && (
              <p role="alert" className="text-danger-text">
                {summaryError}
              </p>
            )}
            {summary && (
              <div className="rounded-control bg-ink p-4">
                <div className="mb-1.5 text-xs text-muted">
                  {summary.kind === "live" ? "Résumé pour les arrivants" : "Bilan de fin de live"} · généré par IA, à relire avant de le partager
                </div>
                <div className="whitespace-pre-wrap leading-relaxed text-soft">{summary.text}</div>
              </div>
            )}
          </>
        )}
      </div>
    </Card>
  );
}
