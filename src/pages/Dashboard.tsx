import { ChatMessage } from "../components/ChatMessage";
import { Button, Card, PageHeader } from "../components/ui";
import { PlatformBadge, PLATFORM_KEYS, PLATFORMS } from "../lib/platforms";
import { FLAGGED, MESSAGES, VIEWERS } from "../lib/mock";

const HEALTH: Record<string, { detail: string; warn?: boolean }> = {
  twitch: { detail: "6,1 Mb/s · 0 image perdue" },
  youtube: { detail: "6,0 Mb/s · latence faible" },
  kick: { detail: "Débit instable · 12 images perdues", warn: true },
  tiktok: { detail: "Lecture du chat uniquement" },
};

export default function Dashboard({ go }: { go: (id: "chat" | "moderation") => void }) {
  const total = Object.values(VIEWERS).reduce((a, b) => a + b, 0);
  const stats = [
    { label: "Spectateurs cumulés", value: total.toLocaleString("fr-FR"), hint: "+ 6 % sur 10 min", good: true },
    { label: "Messages / minute", value: "138", hint: "4 chats fusionnés" },
    { label: "Débit sortant", value: "6,1 Mb/s", hint: "Stable", good: true },
    { label: "À modérer", value: String(FLAGGED.length), hint: "Ouvrir la file →", warn: true, onClick: () => go("moderation") },
  ];

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Tableau de bord"
        subtitle="Vous êtes en direct sur 3 plateformes"
        actions={
          <>
            <span className="flex items-center gap-2 rounded-full border border-[#5a2229] bg-[#2a1216] px-3.5 py-2 font-semibold text-[#ff8a8a]">
              <span className="h-2 w-2 rounded-full bg-youtube" /> EN DIRECT · 02:14:37
            </span>
            <Button variant="danger" className="min-h-11">
              Arrêter le live
            </Button>
          </>
        }
      />

      <div className="mb-5 grid grid-cols-4 gap-4">
        {stats.map((s) => (
          <Card key={s.label} className="p-4.5">
            <div className="text-muted">{s.label}</div>
            <div className={`mt-1.5 text-3xl font-bold ${s.warn ? "text-amber-400" : ""}`}>{s.value}</div>
            {s.onClick ? (
              <button onClick={s.onClick} className="mt-0.5 text-violet-400">
                {s.hint}
              </button>
            ) : (
              <div className={`mt-0.5 ${s.good ? "text-green-400" : "text-muted"}`}>{s.hint}</div>
            )}
          </Card>
        ))}
      </div>

      <div className="flex min-h-0 flex-1 gap-5">
        <section className="flex min-w-0 flex-[1.25] flex-col gap-4">
          <div className="relative flex h-60 items-center justify-center rounded-2xl border border-line bg-gradient-to-br from-[#1e1b3a] via-[#12121c] to-[#1a1226]">
            <span className="absolute left-4 top-3.5 text-[13px] text-muted">Aperçu · 1080p60 · OBS</span>
            <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="#6b6b85" strokeWidth="1.5" aria-hidden="true">
              <rect x="2" y="5" width="14" height="14" rx="2" />
              <path d="M16 10l6-3v10l-6-3" />
            </svg>
          </div>
          <Card className="px-4.5 py-1.5">
            {PLATFORM_KEYS.map((p, i) => (
              <div key={p} className={`flex items-center gap-3.5 py-3.5 ${i < PLATFORM_KEYS.length - 1 ? "border-b border-[#20202c]" : ""}`}>
                <PlatformBadge platform={p} size={30} />
                <div className="flex-1">
                  <div className="font-semibold">{PLATFORMS[p].label}</div>
                  <div className={`text-[13px] ${HEALTH[p].warn ? "text-amber-400" : "text-muted"}`}>{HEALTH[p].detail}</div>
                </div>
                <div className="text-right">
                  <div className="font-semibold">{VIEWERS[p].toLocaleString("fr-FR")}</div>
                  <div className="text-xs text-muted">spectateurs</div>
                </div>
              </div>
            ))}
          </Card>
        </section>

        <Card className="flex min-w-0 flex-1 flex-col overflow-hidden">
          <div className="flex items-center justify-between border-b border-[#20202c] px-4.5 py-4">
            <span className="font-semibold">Chat unifié</span>
            <button onClick={() => go("chat")} className="text-violet-400">
              Agrandir →
            </button>
          </div>
          <div className="flex min-h-0 flex-1 flex-col justify-end gap-0.5 overflow-hidden p-2">
            {MESSAGES.slice(0, 7).map((m) => (
              <ChatMessage key={m.id} message={m} />
            ))}
          </div>
        </Card>
      </div>
    </div>
  );
}
