import { ChatMessage } from "../components/ChatMessage";
import { Badge, Card, CardHeader, LinkButton, LiveBadge, PageHeader } from "../components/ui";
import { PlatformBadge, PLATFORM_KEYS, PLATFORMS, platformKey } from "../lib/platforms";
import { flaggedQueue, formatAge, useStore } from "../lib/store";
import { estimateUnique, viewersByPlatform } from "../lib/viewers";

type Go = (id: "chat" | "moderation" | "accounts") => void;

export default function Dashboard({ go }: { go: Go }) {
  const { messages, channels, connected, infos, msgPerMin, accounts, overlap } = useStore();
  const live = infos.length > 0;
  const estimate = estimateUnique(viewersByPlatform(infos, channels), overlap);
  const viewers = estimate.total;
  const multi = infos.length > 1 && estimate.reliable;
  const online = channels.filter((c) => connected[c.id]).length;
  const flagged = flaggedQueue(messages).length;

  const stats = [
    { label: "Spectateurs cumulés", value: live ? viewers.toLocaleString("fr-FR") : "—", hint: !live ? "Aucun live détecté" : multi ? `≈ ${estimate.unique.toLocaleString("fr-FR")} uniques (estimation)` : "Somme brute · mise à jour toutes les 30 s", title: multi ? `Estimation du dédoublonnage : ${Math.round(estimate.overlapRate * 100)} % des chatteurs sont sur plusieurs plateformes. Fourchette ${estimate.low.toLocaleString("fr-FR")} à ${estimate.total.toLocaleString("fr-FR")}.` : undefined },
    { label: "Messages / minute", value: String(msgPerMin), hint: `${channels.length} chaîne${channels.length > 1 ? "s" : ""} fusionnée${channels.length > 1 ? "s" : ""}` },
    { label: "Chats connectés", value: `${online} / ${channels.length}`, hint: accounts.length ? "Compte Twitch connecté" : "Lecture seule : aucun compte", good: online === channels.length && channels.length > 0 },
    { label: "À modérer", value: String(flagged), hint: "Ouvrir la file →", warn: flagged > 0, onClick: () => go("moderation") },
  ];

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Tableau de bord"
        subtitle={live ? `Vous êtes en direct sur ${infos.length} chaîne${infos.length > 1 ? "s" : ""}` : "Aucun live en cours sur vos chaînes"}
        actions={live ? <LiveBadge>EN DIRECT · {formatAge(Math.min(...infos.map((i) => Date.parse(i.started_at))))}</LiveBadge> : undefined}
      />

      <div className="mb-5 grid grid-cols-4 gap-4">
        {stats.map((s) => (
          <Card key={s.label} className="p-5" title={s.title}>
            <div className="text-muted">{s.label}</div>
            <div className={`mt-1.5 text-3xl font-bold ${s.warn ? "text-warn" : ""}`}>{s.value}</div>
            {s.onClick ? (
              <LinkButton onClick={s.onClick} className="mt-0.5">
                {s.hint}
              </LinkButton>
            ) : (
              <div className={`mt-0.5 ${s.good ? "text-success" : "text-muted"}`}>{s.hint}</div>
            )}
          </Card>
        ))}
      </div>

      <div className="flex min-h-0 flex-1 gap-5">
        <section className="flex min-w-0 flex-[1.25] flex-col gap-4">
          <Card className="px-5 py-1.5">
            {PLATFORM_KEYS.map((p, i) => {
              const mine = channels.filter((c) => platformKey(c.platform) === p);
              const platformViewers = infos.filter((info) => mine.some((c) => c.id === info.channel_id)).reduce((n, info) => n + info.viewers, 0);
              const supported = p === "twitch";
              return (
                <div key={p} className={`flex items-center gap-3.5 py-3.5 ${i < PLATFORM_KEYS.length - 1 ? "border-b border-line" : ""}`}>
                  <PlatformBadge platform={p} size={30} />
                  <div className="flex-1">
                    <div className="font-semibold">{PLATFORMS[p].label}</div>
                    <div className="text-[13px] text-muted">
                      {!supported ? "Bientôt disponible" : mine.length === 0 ? "Aucune chaîne suivie" : mine.map((c) => c.display_name).join(", ")}
                    </div>
                  </div>
                  {supported && mine.length > 0 ? (
                    <div className="text-right">
                      <div className="font-semibold">{platformViewers.toLocaleString("fr-FR")}</div>
                      <div className="text-xs text-muted">spectateurs</div>
                    </div>
                  ) : (
                    supported && <LinkButton onClick={() => go("accounts")}>Ajouter →</LinkButton>
                  )}
                  {!supported && <Badge tone="info">Bientôt</Badge>}
                </div>
              );
            })}
          </Card>
        </section>

        <Card className="flex min-w-0 flex-1 flex-col overflow-hidden">
          <CardHeader title="Chat unifié" aside={<LinkButton onClick={() => go("chat")}>Agrandir →</LinkButton>} />
          <div className="flex min-h-0 flex-1 flex-col justify-end gap-0.5 overflow-hidden p-2">
            {messages.length === 0 && <p className="m-auto text-muted">Aucun message pour l'instant.</p>}
            {messages.slice(-7).map((m) => (
              <ChatMessage key={m.id} message={m} />
            ))}
          </div>
        </Card>
      </div>
    </div>
  );
}
