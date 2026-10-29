import { AiSummaryCard } from "../components/AiSummaryCard";
import { ChatMessage } from "../components/ChatMessage";
import { StreamInfoForm } from "../components/StreamInfoForm";
import { Badge, Card, CardHeader, LinkButton, LiveBadge, PageHeader } from "../components/ui";
import { PlatformBadge, PLATFORM_KEYS, PLATFORMS, platformKey } from "../lib/platforms";
import { localeOf } from "../lib/i18n";
import { flaggedQueue, formatAge, useStore } from "../lib/store";
import { estimateUnique, viewersByPlatform } from "../lib/viewers";

type Go = (id: "chat" | "moderation" | "accounts") => void;

export default function Dashboard({ go }: { go: Go }) {
  const { messages, channels, connected, infos, msgPerMin, accounts, overlap, t, lang } = useStore();
  const nf = (n: number) => n.toLocaleString(localeOf(lang));
  const live = infos.length > 0;
  const estimate = estimateUnique(viewersByPlatform(infos, channels), overlap);
  const viewers = estimate.total;
  const multi = infos.length > 1 && estimate.reliable;
  const online = channels.filter((c) => connected[c.id]).length;
  const flagged = flaggedQueue(messages).length;

  const stats = [
    { label: t("dash.viewers"), value: live ? nf(viewers) : "—", hint: !live ? t("dash.viewersHintOff") : multi ? t("dash.uniquesEstimate", { n: nf(estimate.unique) }) : t("dash.viewersHintLive"), title: multi ? t("dash.overlapTitle", { pct: Math.round(estimate.overlapRate * 100), low: nf(estimate.low), high: nf(estimate.total) }) : undefined },
    { label: t("dash.msgPerMin"), value: String(msgPerMin), hint: t("dash.merged", { n: channels.length }) },
    { label: t("dash.connected"), value: `${online} / ${channels.length}`, hint: t(accounts.length ? "dash.withAccount" : "dash.readOnly"), good: online === channels.length && channels.length > 0 },
    { label: t("dash.toModerate"), value: String(flagged), hint: t("dash.openQueue"), warn: flagged > 0, onClick: () => go("moderation") },
  ];

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title={t("dash.title")}
        subtitle={live ? t("dash.live", { n: infos.length }) : t("dash.offline")}
        actions={live ? <LiveBadge>{t("common.live")} · {formatAge(Math.min(...infos.map((i) => Date.parse(i.started_at))))}</LiveBadge> : undefined}
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
              const supported = p === "twitch" || p === "trovo" || p === "youtube" || p === "kick";
              return (
                <div key={p} className={`flex items-center gap-3.5 py-3.5 ${i < PLATFORM_KEYS.length - 1 ? "border-b border-line" : ""}`}>
                  <PlatformBadge platform={p} size={30} />
                  <div className="flex-1">
                    <div className="font-semibold">{PLATFORMS[p].label}</div>
                    <div className="text-[13px] text-muted">
                      {!supported ? t("dash.comingSoon") : mine.length === 0 ? t("dash.noChannel") : mine.map((c) => c.display_name).join(", ")}
                    </div>
                  </div>
                  {supported && mine.length > 0 ? (
                    <div className="text-right">
                      <div className="font-semibold">{nf(platformViewers)}</div>
                      <div className="text-xs text-muted">{t("dash.viewersUnit")}</div>
                    </div>
                  ) : (
                    supported && <LinkButton onClick={() => go("accounts")}>{t("dash.add")}</LinkButton>
                  )}
                  {!supported && <Badge tone="info">{t("common.soon")}</Badge>}
                </div>
              );
            })}
          </Card>
          <AiSummaryCard goModeration={() => go("moderation")} />
          <StreamInfoForm />
        </section>

        <Card className="flex min-w-0 flex-1 flex-col overflow-hidden">
          <CardHeader title={t("dash.unifiedChat")} aside={<LinkButton onClick={() => go("chat")}>{t("dash.expand")}</LinkButton>} />
          <div className="flex min-h-0 flex-1 flex-col justify-end gap-0.5 overflow-hidden p-2">
            {messages.length === 0 && <p className="m-auto text-muted">{t("dash.noMessages")}</p>}
            {messages.slice(-7).map((m) => (
              <ChatMessage key={m.id} message={m} />
            ))}
          </div>
        </Card>
      </div>
    </div>
  );
}
