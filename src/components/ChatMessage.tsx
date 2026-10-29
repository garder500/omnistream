import { PlatformBadge, PLATFORMS, platformKey } from "../lib/platforms";
import { useStore, type StoredMessage } from "../lib/store";
import { Badge } from "./ui";
import type { EmoteSpan } from "../lib/types";

const EXTRA_BADGES: Record<string, string> = {
  broadcaster: "Streamer",
  vip: "VIP",
  founder: "Fondateur",
  staff: "Staff",
  partner: "Partenaire",
};

/** Découpe le texte en fragments texte / émotes (indices en caractères Unicode). */
function renderText(text: string, emotes: EmoteSpan[] | undefined) {
  if (!emotes?.length) return text;
  const chars = Array.from(text);
  const out: React.ReactNode[] = [];
  let pos = 0;
  [...emotes]
    .sort((a, b) => a.start - b.start)
    .forEach((e, i) => {
      if (e.start < pos || e.end > chars.length) return;
      if (e.start > pos) out.push(chars.slice(pos, e.start).join(""));
      out.push(
        <img
          key={i}
          src={e.url}
          alt={e.name}
          title={e.name}
          loading="lazy"
          className={`inline-block h-7 w-auto align-middle ${e.zero_width ? "-ml-6" : "mx-0.5"}`}
        />,
      );
      pos = e.end;
    });
  if (pos < chars.length) out.push(chars.slice(pos).join(""));
  return out;
}

export function ChatMessage({ message, actions }: { message: StoredMessage; actions?: boolean }) {
  const { moderate, channels } = useStore();
  const platform = platformKey(message.platform);
  const meta = PLATFORMS[platform];
  const channel = channels.find((c) => c.id === message.channel_id);
  const author = message.author;

  return (
    <div className="group flex gap-2.5 rounded-lg px-2.5 py-2 hover:bg-raised">
      <span className="mt-0.5">
        <PlatformBadge platform={platform} />
      </span>
      <div className="min-w-0 flex-1 leading-snug">
        {channel && <span className="mr-1.5 text-xs text-muted">#{channel.display_name}</span>}
        <b className={meta.text}>{author.display_name}</b>{" "}
        {author.is_moderator && (
          <span className="mr-1 align-middle">
            <Badge>Modo</Badge>
          </span>
        )}
        {author.is_subscriber && (
          <span className="mr-1 align-middle">
            <Badge tone="brand">Abo</Badge>
          </span>
        )}
        {message.badges
          ?.filter((b) => EXTRA_BADGES[b.set_id])
          .map((b) => (
            <span key={b.set_id} className="mr-1 align-middle">
              <Badge>{EXTRA_BADGES[b.set_id]}</Badge>
            </span>
          ))}
        <span className={message.deleted ? "text-muted line-through" : "text-zinc-300"}>
          {renderText(message.text, message.emotes)}
        </span>
        {message.deleted && <span className="ml-2 text-xs text-muted">supprimé</span>}
        {actions && !meta.readOnly && !message.deleted && !author.is_moderator && (
          <div className="mt-2 hidden gap-2 group-hover:flex">
            <ActionButton onClick={() => moderate(message, { kind: "delete_message", message_id: message.id }, "Message supprimé")}>
              Supprimer
            </ActionButton>
            <ActionButton onClick={() => moderate(message, { kind: "timeout", user_id: author.external_id, seconds: 600 }, "Timeout 10 min")}>
              Timeout 10 min
            </ActionButton>
            <ActionButton onClick={() => moderate(message, { kind: "ban", user_id: author.external_id }, "Banni")}>Bannir</ActionButton>
          </div>
        )}
      </div>
    </div>
  );
}

function ActionButton({ onClick, children }: { onClick: () => void; children: React.ReactNode }) {
  return (
    <button onClick={onClick} className="min-h-9 rounded-lg bg-raised px-3.5 text-xs font-medium hover:bg-line-strong">
      {children}
    </button>
  );
}
