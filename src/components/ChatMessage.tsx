import { PlatformBadge, PLATFORMS, platformKey } from "../lib/platforms";
import { useStore, type StoredMessage } from "../lib/store";
import { Badge } from "./ui";

export function ChatMessage({ message, actions }: { message: StoredMessage; actions?: boolean }) {
  const { moderate, channels, t } = useStore();
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
            <Badge>{t("common.mod")}</Badge>
          </span>
        )}
        {author.is_subscriber && (
          <span className="mr-1 align-middle">
            <Badge tone="brand">{t("common.sub")}</Badge>
          </span>
        )}
        <span className={message.deleted ? "text-muted line-through" : "text-zinc-300"}>{message.text}</span>
        {message.deleted && <span className="ml-2 text-xs text-muted">{t("common.deleted")}</span>}
        {actions && !meta.readOnly && !message.deleted && !author.is_moderator && (
          <div className="mt-2 hidden gap-2 group-hover:flex">
            <ActionButton onClick={() => moderate(message, { kind: "delete_message", message_id: message.id }, t("common.messageDeleted"))}>
              {t("common.delete")}
            </ActionButton>
            <ActionButton onClick={() => moderate(message, { kind: "timeout", user_id: author.external_id, seconds: 600 }, t("common.timeout10"))}>
              {t("common.timeout10")}
            </ActionButton>
            <ActionButton onClick={() => moderate(message, { kind: "ban", user_id: author.external_id }, t("common.banned"))}>{t("common.ban")}</ActionButton>
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
