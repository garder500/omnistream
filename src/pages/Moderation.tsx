import { useState } from "react";
import { Button, Card, PageHeader, PlatformChipGroup } from "../components/ui";
import { PlatformBadge, PLATFORMS, type PlatformKey } from "../lib/platforms";
import { FLAGGED } from "../lib/mock";

const ACTIONABLE: PlatformKey[] = ["twitch", "youtube", "kick"];

export default function Moderation() {
  const [queue, setQueue] = useState(FLAGGED);
  const [selectedId, setSelectedId] = useState(FLAGGED[0]?.id);
  const [targets, setTargets] = useState<PlatformKey[]>(["kick", "twitch"]);
  const [log, setLog] = useState<string[]>([]);
  const selected = queue.find((m) => m.id === selectedId) ?? queue[0];

  const resolve = (verb: string) => {
    if (!selected) return;
    const where = targets.map((t) => PLATFORMS[t].label).join(", ") || "aucune plateforme";
    setLog((l) => [`${verb} · ${selected.author} · ${where}`, ...l].slice(0, 5));
    const rest = queue.filter((m) => m.id !== selected.id);
    setQueue(rest);
    setSelectedId(rest[0]?.id);
  };

  const toggle = (p: PlatformKey) => setTargets((t) => (t.includes(p) ? t.filter((x) => x !== p) : [...t, p]));

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Modération"
        subtitle="Une action, appliquée là où vous le choisissez"
        actions={
          <>
            <Button variant="outline">Mode lent : désactivé</Button>
            <Button variant="outline">Abonnés uniquement</Button>
          </>
        }
      />

      <div className="flex min-h-0 flex-1 gap-5">
        <Card className="flex min-w-0 flex-1 flex-col overflow-hidden">
          <div className="flex gap-1.5 border-b border-line p-3.5">
            <span className="rounded-full bg-brand-soft px-3.5 py-1.5 font-semibold text-[#c9a8ff]">À traiter · {queue.length}</span>
            <span className="px-3.5 py-1.5 text-muted">Traités · {27 + log.length}</span>
            <span className="px-3.5 py-1.5 text-muted">Bannis · 9</span>
          </div>
          <div className="flex flex-col gap-2 overflow-auto p-2.5">
            {queue.length === 0 && <p className="p-6 text-center text-muted">File vide : rien à modérer.</p>}
            {queue.map((m) => (
              <button
                key={m.id}
                onClick={() => setSelectedId(m.id)}
                aria-pressed={m.id === selected?.id}
                className={`rounded-control border p-4 text-left ${m.id === selected?.id ? "border-brand-line bg-raised" : "border-line bg-panel hover:bg-raised"}`}
              >
                <div className="flex items-center gap-2.5">
                  <PlatformBadge platform={m.platform} />
                  <b>{m.author}</b>
                  <span className="text-muted">· il y a {m.age}</span>
                  <span className="ml-auto rounded-md bg-warn-soft px-2 py-0.5 text-xs font-semibold text-warn">{m.reason}</span>
                </div>
                <div className="mt-2 text-zinc-300">{m.text}</div>
              </button>
            ))}
          </div>
          <div className="mt-auto border-t border-line px-4.5 py-3.5">
            <div className="mb-2 font-semibold">Journal des actions</div>
            {log.length === 0 && <div className="text-muted">Aucune action pour l'instant.</div>}
            {log.map((l, i) => (
              <div key={i} className="py-1 text-soft">
                {l}
              </div>
            ))}
          </div>
        </Card>

        <Card className="flex w-105 shrink-0 flex-col gap-4 p-5">
          {selected ? (
            <>
              <div className="flex items-center gap-3.5">
                <div className="flex h-13 w-13 items-center justify-center rounded-full bg-raised text-xl font-bold">
                  {selected.author[0].toUpperCase()}
                </div>
                <div>
                  <div className="text-xl font-bold">{selected.author}</div>
                  <div className="text-muted">Signalé : {selected.reason.toLowerCase()}</div>
                </div>
              </div>
              <div className="rounded-control bg-ink p-3 leading-relaxed text-soft">« {selected.text} »</div>
              <div className="flex flex-col gap-3">
                <PlatformChipGroup legend="Appliquer à" platforms={ACTIONABLE} selected={targets} onToggle={toggle} />
                <p className="text-xs text-muted">TikTok : lecture seule, pas de modération possible.</p>
              </div>
              <div className="mt-auto grid grid-cols-2 gap-2.5">
                <Button onClick={() => resolve("Message supprimé")}>Supprimer le message</Button>
                <Button variant="warn" onClick={() => resolve("Timeout 10 min")}>
                  Timeout 10 min
                </Button>
                <Button variant="danger" className="col-span-2" onClick={() => resolve("Banni")}>
                  Bannir sur {targets.length} plateforme{targets.length > 1 ? "s" : ""}
                </Button>
              </div>
            </>
          ) : (
            <p className="m-auto text-muted">Sélectionnez un message à traiter.</p>
          )}
        </Card>
      </div>
    </div>
  );
}
