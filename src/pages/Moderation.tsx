import { useState } from "react";
import AutomodRules from "../components/AutomodRules";
import { Avatar, Button, Card, PageHeader } from "../components/ui";
import { PlatformBadge, platformKey } from "../lib/platforms";
import { flaggedQueue, formatAge, useStore } from "../lib/store";

export default function Moderation() {
  const { messages, modLog, channels, moderate, dismiss } = useStore();
  const queue = flaggedQueue(messages);
  const [selectedId, setSelectedId] = useState<string>();
  const selected = queue.find((m) => m.id === selectedId) ?? queue[0];
  const channel = channels.find((c) => c.id === selected?.channel_id);
  const [tab, setTab] = useState<"queue" | "rules">("queue");

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Modération"
        subtitle="Les messages qui déclenchent une règle d'automod arrivent ici en temps réel"
        actions={
          <div role="tablist" className="flex gap-1.5">
            {(["queue", "rules"] as const).map((t) => (
              <button
                key={t}
                role="tab"
                aria-selected={tab === t}
                onClick={() => setTab(t)}
                className={`rounded-full px-4 py-2 font-semibold ${tab === t ? "bg-brand-soft text-brand-ring" : "text-muted hover:text-zinc-100"}`}
              >
                {t === "queue" ? "File" : "Règles d'automod"}
              </button>
            ))}
          </div>
        }
      />

      {tab === "rules" ? (
        <AutomodRules />
      ) : (
      <div className="flex min-h-0 flex-1 gap-5">
        <Card className="flex min-w-0 flex-1 flex-col overflow-hidden">
          <div className="flex gap-1.5 border-b border-line p-3.5">
            <span className="rounded-full bg-brand-soft px-3.5 py-1.5 font-semibold text-brand-ring">À traiter · {queue.length}</span>
            <span className="px-3.5 py-1.5 text-muted">Actions cette session · {modLog.length}</span>
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
                  <PlatformBadge platform={platformKey(m.platform)} />
                  <b>{m.author.display_name}</b>
                  <span className="text-muted">· il y a {formatAge(m.timestamp_ms)}</span>
                  <span className="ml-auto rounded-md bg-warn-soft px-2 py-0.5 text-xs font-semibold text-warn">{m.flag}</span>
                </div>
                <div className="mt-2 text-zinc-300">{m.text}</div>
              </button>
            ))}
          </div>
          <div className="mt-auto border-t border-line px-4.5 py-3.5">
            <div className="mb-2 font-semibold">Journal des actions</div>
            {modLog.length === 0 && <div className="text-muted">Aucune action pour l'instant.</div>}
            {modLog.slice(0, 5).map((l, i) => (
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
                <Avatar name={selected.author.display_name} size={52} />
                <div>
                  <div className="text-xl font-bold">{selected.author.display_name}</div>
                  <div className="text-muted">
                    Signalé : {selected.flag?.toLowerCase()}
                    {channel && ` · #${channel.display_name}`}
                  </div>
                </div>
              </div>
              <div className="rounded-control bg-ink p-3 leading-relaxed text-soft">« {selected.text} »</div>
              <p className="text-xs text-muted">L'action est appliquée sur la chaîne où le message a été écrit.</p>
              <div className="mt-auto grid grid-cols-2 gap-2.5">
                <Button onClick={() => moderate(selected, { kind: "delete_message", message_id: selected.id }, "Message supprimé")}>
                  Supprimer le message
                </Button>
                <Button variant="warn" onClick={() => moderate(selected, { kind: "timeout", user_id: selected.author.external_id, seconds: 600 }, "Timeout 10 min")}>
                  Timeout 10 min
                </Button>
                <Button variant="danger" onClick={() => moderate(selected, { kind: "ban", user_id: selected.author.external_id }, "Banni")}>
                  Bannir
                </Button>
                <Button variant="outline" onClick={() => dismiss(selected.id)}>
                  Ignorer
                </Button>
              </div>
            </>
          ) : (
            <p className="m-auto text-muted">Sélectionnez un message à traiter.</p>
          )}
        </Card>
      </div>
      )}
    </div>
  );
}
