import { useEffect, useRef, useState } from "react";
import { ChatMessage } from "../components/ChatMessage";
import { Button, Card, Input, LiveBadge } from "../components/ui";
import { PlatformBadge, PLATFORM_KEYS, PLATFORMS, platformKey, type PlatformKey } from "../lib/platforms";
import { formatAge, useStore } from "../lib/store";
import type { StreamEvent } from "../lib/types";

function describe(e: StreamEvent): string {
  switch (e.type) {
    case "sub":
      return e.months > 1 ? `s'abonne · ${e.months} mois` : "s'abonne";
    case "gift":
      return e.recipient ? `offre un abonnement à ${e.recipient}` : "offre un abonnement";
    case "raid":
      return `raid avec ${e.viewers} viewers`;
  }
}

export default function MultiChat({ go }: { go: (id: "accounts") => void }) {
  const { messages, events, channels, infos, msgPerMin, send } = useStore();
  const [filter, setFilter] = useState<PlatformKey | "all">("all");
  const [draft, setDraft] = useState("");
  const bottom = useRef<HTMLDivElement>(null);
  const shown = messages.filter((m) => filter === "all" || platformKey(m.platform) === filter);
  const total = infos.reduce((n, i) => n + i.viewers, 0);
  const live = infos[0];

  useEffect(() => {
    bottom.current?.scrollIntoView({ block: "end" });
  }, [shown.length]);

  const sendable = channels.filter((c) => !PLATFORMS[platformKey(c.platform)].readOnly);

  return (
    <div className="flex h-full flex-col gap-4">
      {live && (
        <Card className="flex flex-wrap items-center gap-4 px-5 py-3">
          <LiveBadge>EN DIRECT {formatAge(Date.parse(live.started_at))}</LiveBadge>
          <div className="min-w-0 flex-1 truncate text-[15px] font-semibold">{live.title}</div>
          <span className="flex items-center gap-2">
            <PlatformBadge platform="twitch" size={24} />
            <b>{total.toLocaleString("fr-FR")}</b> spectateurs
          </span>
        </Card>
      )}

      <div className="flex min-h-0 flex-1 gap-4">
        <Card className="flex min-w-0 flex-1 flex-col overflow-hidden">
          <div className="flex flex-wrap items-center gap-2 border-b border-line px-3.5 py-2.5">
            <span className="mr-1.5 font-semibold">Chat</span>
            <FilterChip active={filter === "all"} onClick={() => setFilter("all")}>
              Tous
            </FilterChip>
            {PLATFORM_KEYS.map((p) => (
              <FilterChip key={p} active={filter === p} onClick={() => setFilter(p)}>
                <b className={PLATFORMS[p].text}>{PLATFORMS[p].letter}</b> {PLATFORMS[p].label}
              </FilterChip>
            ))}
            <span className="ml-auto text-muted">{msgPerMin} msg/min</span>
          </div>

          <div className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-auto p-1.5 text-[15px]">
            {channels.length === 0 ? (
              <div className="m-auto flex flex-col items-center gap-3 text-center text-muted">
                <p>Aucune chaîne suivie : le chat est vide.</p>
                <Button variant="primary" onClick={() => go("accounts")}>
                  Ajouter une chaîne
                </Button>
              </div>
            ) : shown.length === 0 ? (
              <p className="m-auto text-muted">En attente des premiers messages…</p>
            ) : (
              <div className="mt-auto flex flex-col gap-0.5">
                {shown.map((m) => (
                  <ChatMessage key={m.id} message={m} actions />
                ))}
              </div>
            )}
            <div ref={bottom} />
          </div>

          <form
            className="flex items-center gap-2.5 border-t border-line p-3.5"
            onSubmit={async (e) => {
              e.preventDefault();
              const text = draft.trim();
              if (!text) return;
              setDraft("");
              if (!(await send(sendable.map((c) => c.id), text))) setDraft(text);
            }}
          >
            <Input value={draft} onChange={(e) => setDraft(e.target.value)} placeholder="Écrire un message sur toutes les chaînes suivies…" className="flex-1" />
            <Button variant="primary" type="submit" disabled={!draft.trim() || sendable.length === 0}>
              Envoyer
            </Button>
          </form>
        </Card>

        <aside className="flex w-96 shrink-0 flex-col gap-4">
          <Card className="flex min-h-0 flex-1 flex-col overflow-hidden">
            <div className="flex justify-between border-b border-line px-4 py-3 font-semibold">
              Événements <span className="font-normal text-muted">Cette session</span>
            </div>
            <div className="flex flex-col gap-2 overflow-auto p-2.5">
              {events.length === 0 && <p className="p-4 text-center text-muted">Abonnements et raids apparaîtront ici.</p>}
              {events.map((e) => (
                <div key={e.id} className={`flex items-center gap-3 rounded-control border p-3 ${e.type === "raid" ? "border-warn/40 bg-warn-soft" : "border-brand-line bg-raised"}`}>
                  <PlatformBadge platform={platformKey(e.platform)} size={32} />
                  <div className="min-w-0 flex-1">
                    <div>
                      <b>{e.who}</b> <span className="text-soft">{describe(e)}</span>
                    </div>
                    {e.message && <div className="text-[13px] text-muted">« {e.message} »</div>}
                  </div>
                  <span className="text-xs text-muted">{formatAge(e.timestamp_ms)}</span>
                </div>
              ))}
            </div>
          </Card>
        </aside>
      </div>
    </div>
  );
}

function FilterChip({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      onClick={onClick}
      aria-pressed={active}
      className={`min-h-10 rounded-full px-4 font-medium transition ${active ? "bg-brand text-white" : "border border-line-strong bg-field text-soft hover:bg-raised hover:text-white"}`}
    >
      {children}
    </button>
  );
}
