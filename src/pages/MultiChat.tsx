import { useState } from "react";
import { ChatMessage } from "../components/ChatMessage";
import { Button, Card, CardHeader, Input, LiveBadge } from "../components/ui";
import { PlatformBadge, PLATFORM_KEYS, PLATFORMS, type PlatformKey } from "../lib/platforms";
import { EVENTS, MESSAGES, VIEWERS } from "../lib/mock";

export default function MultiChat() {
  const [filter, setFilter] = useState<PlatformKey | "all">("all");
  const [sendTo, setSendTo] = useState<PlatformKey[]>(["twitch", "youtube", "kick"]);
  const [draft, setDraft] = useState("");
  const total = Object.values(VIEWERS).reduce((a, b) => a + b, 0);
  const messages = MESSAGES.filter((m) => filter === "all" || m.platform === filter);

  const toggle = (p: PlatformKey) => setSendTo((s) => (s.includes(p) ? s.filter((x) => x !== p) : [...s, p]));

  return (
    <div className="flex h-full flex-col gap-4">
      <Card className="flex flex-wrap items-center gap-4 px-5 py-3">
        <LiveBadge>EN DIRECT 02:14:37</LiveBadge>
        <div className="min-w-0 flex-1 truncate text-[15px] font-semibold">Reprise du speedrun · Partie 2</div>
        {PLATFORM_KEYS.map((p) => (
          <span key={p} className="flex items-center gap-2">
            <PlatformBadge platform={p} size={24} />
            <b>{VIEWERS[p].toLocaleString("fr-FR")}</b>
          </span>
        ))}
        <span className="border-l border-line-strong pl-4 text-muted">
          Total <b className="text-zinc-100">{total.toLocaleString("fr-FR")}</b>
        </span>
      </Card>

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
            <span className="ml-auto text-muted">138 msg/min</span>
          </div>

          <div className="flex min-h-0 flex-1 flex-col justify-end gap-0.5 overflow-auto p-1.5 text-[15px]">
            {messages.map((m) => (
              <ChatMessage key={m.id} message={m} actions={!PLATFORMS[m.platform].readOnly} />
            ))}
          </div>

          <form
            className="flex items-center gap-2.5 border-t border-line p-3.5"
            onSubmit={(e) => {
              e.preventDefault();
              setDraft("");
            }}
          >
            <Input value={draft} onChange={(e) => setDraft(e.target.value)} placeholder="Écrire un message…" className="flex-1" />
            <div className="flex gap-2.5" role="group" aria-label="Plateformes d'envoi">
              {PLATFORM_KEYS.filter((p) => !PLATFORMS[p].readOnly).map((p) => (
                <button
                  type="button"
                  key={p}
                  aria-pressed={sendTo.includes(p)}
                  title={PLATFORMS[p].label}
                  onClick={() => toggle(p)}
                  className={sendTo.includes(p) ? "" : "opacity-40"}
                >
                  <PlatformBadge platform={p} size={36} />
                </button>
              ))}
            </div>
            <Button variant="primary" type="submit" disabled={!draft || sendTo.length === 0}>
              Envoyer
            </Button>
          </form>
        </Card>

        <aside className="flex w-96 shrink-0 flex-col gap-4">
          <Card className="flex min-h-0 flex-1 flex-col overflow-hidden">
            <div className="flex justify-between border-b border-line px-4 py-3 font-semibold">
              Événements <span className="font-normal text-muted">Aujourd'hui</span>
            </div>
            <div className="flex flex-col gap-2 overflow-auto p-2.5">
              {EVENTS.map((e) => (
                <div
                  key={e.id}
                  className={`flex items-center gap-3 rounded-control border p-3 ${
                    e.highlight === "donation"
                      ? "border-warn/40 bg-warn-soft"
                      : e.highlight === "sub"
                        ? "border-brand-line bg-raised"
                        : "border-line bg-panel"
                  }`}
                >
                  <PlatformBadge platform={e.platform} size={32} />
                  <div className="min-w-0 flex-1">
                    <div>
                      <b>{e.who}</b> <span className="text-soft">{e.what}</span>
                    </div>
                    {e.detail && <div className="text-[13px] text-muted">{e.detail}</div>}
                  </div>
                  <span className="text-xs text-muted">{e.age}</span>
                </div>
              ))}
            </div>
          </Card>

          <Card className="p-4">
            <div className="mb-1.5 font-semibold">Overlay pour OBS</div>
            <p className="mb-3 text-muted">Affichez le chat fusionné et les alertes dans votre scène.</p>
            <div className="flex items-center gap-2">
              <code className="min-w-0 flex-1 truncate rounded-lg border border-line-strong bg-ink px-3 py-2.5 text-xs text-soft">
                http://127.0.0.1:8080/overlay/chat
              </code>
              <Button variant="primary" onClick={() => navigator.clipboard?.writeText("http://127.0.0.1:8080/overlay/chat")}>
                Copier
              </Button>
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
