import { useMemo, useState, type ReactNode } from "react";
import { PlatformBadge, PLATFORM_KEYS, PLATFORMS, platformKey, type PlatformKey } from "../lib/platforms";
import { formatAge } from "../lib/store";
import type { StreamEvent, StreamEventType } from "../lib/types";

export const EVENT_TYPES: { id: StreamEventType; label: string; plural: string }[] = [
  { id: "follow", label: "Follow", plural: "Follows" },
  { id: "sub", label: "Abonnement", plural: "Abonnements" },
  { id: "gift", label: "Sub offert", plural: "Subs offerts" },
  { id: "raid", label: "Raid", plural: "Raids" },
  { id: "donation", label: "Don", plural: "Dons" },
  { id: "bits", label: "Bits", plural: "Bits" },
];

export function eventLabel(type: StreamEventType, count = 2): string {
  const t = EVENT_TYPES.find((x) => x.id === type);
  return (count === 1 ? t?.label : t?.plural) ?? type;
}

export function formatMoney(cents: number, currency: string): string {
  try {
    return new Intl.NumberFormat("fr-FR", { style: "currency", currency }).format(cents / 100);
  } catch {
    // Code de devise inconnu de l'environnement.
    return `${(cents / 100).toLocaleString("fr-FR", { minimumFractionDigits: 2 })} ${currency}`;
  }
}

export function describeEvent(e: StreamEvent): string {
  switch (e.type) {
    case "follow":
      return "vous suit";
    case "sub":
      return e.months > 1 ? `s'abonne · ${e.months} mois` : "s'abonne";
    case "gift":
      return e.recipient ? `offre un abonnement à ${e.recipient}` : "offre un abonnement";
    case "raid":
      return `raid avec ${e.viewers.toLocaleString("fr-FR")} viewers`;
    case "donation":
      return `fait un don de ${formatMoney(e.amount_cents, e.currency)}`;
    case "bits":
      return `offre ${e.amount.toLocaleString("fr-FR")} bits`;
  }
}

/** Les événements qui rapportent de l'argent ou du monde ressortent du fil. */
const HIGHLIGHT: StreamEventType[] = ["raid", "donation"];

interface Props {
  events: StreamEvent[];
  title?: string;
  aside?: ReactNode;
  emptyText?: string;
  /** Date absolue plutôt que « il y a » (historique). */
  absoluteTime?: boolean;
  className?: string;
}

/** Fil d'événements unifié (follow, sub, gift, raid, don, bits) avec filtres plateforme et type. */
export default function EventFeed({ events, title = "Événements", aside, emptyText = "Les événements de vos lives apparaîtront ici.", absoluteTime, className = "" }: Props) {
  const [platform, setPlatform] = useState<PlatformKey | "all">("all");
  const [type, setType] = useState<StreamEventType | "all">("all");

  const shown = useMemo(
    () => events.filter((e) => (platform === "all" || platformKey(e.platform) === platform) && (type === "all" || e.type === type)),
    [events, platform, type],
  );
  // Seuls les filtres utiles : plateformes et types réellement présents.
  const platformsSeen = useMemo(() => PLATFORM_KEYS.filter((p) => events.some((e) => platformKey(e.platform) === p)), [events]);
  const typesSeen = useMemo(() => EVENT_TYPES.filter((t) => events.some((e) => e.type === t.id)), [events]);

  return (
    <section className={`flex min-h-0 flex-col overflow-hidden rounded-card border border-line bg-panel ${className}`} aria-label={title}>
      <div className="flex items-center justify-between border-b border-line px-4 py-3 font-semibold">
        {title}
        {aside ?? <span className="font-normal text-muted">{shown.length === events.length ? events.length : `${shown.length} / ${events.length}`}</span>}
      </div>

      {events.length > 0 && (
        <div className="flex flex-col gap-2 border-b border-line px-3 py-2.5">
          <ChipRow label="Plateforme">
            <Chip active={platform === "all"} onClick={() => setPlatform("all")}>
              Toutes
            </Chip>
            {platformsSeen.map((p) => (
              <Chip key={p} active={platform === p} onClick={() => setPlatform(p)}>
                <b className={PLATFORMS[p].text}>{PLATFORMS[p].letter}</b> {PLATFORMS[p].label}
              </Chip>
            ))}
          </ChipRow>
          <ChipRow label="Type">
            <Chip active={type === "all"} onClick={() => setType("all")}>
              Tous
            </Chip>
            {typesSeen.map((t) => (
              <Chip key={t.id} active={type === t.id} onClick={() => setType(t.id)}>
                {t.label}
              </Chip>
            ))}
          </ChipRow>
        </div>
      )}

      <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-auto p-2.5">
        {events.length === 0 && <p className="p-4 text-center text-muted">{emptyText}</p>}
        {events.length > 0 && shown.length === 0 && <p className="p-4 text-center text-muted">Aucun événement pour ces filtres.</p>}
        {shown.map((e) => (
          <div
            key={`${e.id}:${e.timestamp_ms}:${e.who}`}
            className={`flex items-center gap-3 rounded-control border p-3 ${HIGHLIGHT.includes(e.type) ? "border-warn/40 bg-warn-soft" : "border-brand-line bg-raised"}`}
          >
            <PlatformBadge platform={platformKey(e.platform)} size={32} />
            <div className="min-w-0 flex-1">
              <div>
                <b>{e.who}</b> <span className="text-soft">{describeEvent(e)}</span>
              </div>
              {e.message && <div className="break-words text-[13px] text-muted">« {e.message} »</div>}
            </div>
            <span className="shrink-0 text-xs text-muted">
              {absoluteTime ? new Date(e.timestamp_ms).toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit", second: "2-digit" }) : formatAge(e.timestamp_ms)}
            </span>
          </div>
        ))}
      </div>
    </section>
  );
}

function ChipRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div role="group" aria-label={label} className="flex flex-wrap items-center gap-1.5">
      {children}
    </div>
  );
}

function Chip({ active, onClick, children }: { active: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={active}
      className={`min-h-9 rounded-full px-3.5 text-[13px] font-medium transition ${active ? "bg-brand text-white" : "border border-line-strong bg-field text-soft hover:bg-raised hover:text-white"}`}
    >
      {children}
    </button>
  );
}
