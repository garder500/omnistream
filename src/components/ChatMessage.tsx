import { PlatformBadge, PLATFORMS } from "../lib/platforms";
import type { MockMessage } from "../lib/mock";

export function ChatMessage({ message, actions }: { message: MockMessage; actions?: boolean }) {
  const meta = PLATFORMS[message.platform];
  return (
    <div className="group flex gap-2.5 rounded-lg px-2.5 py-2 hover:bg-[#1b1b28]">
      <span className="mt-0.5">
        <PlatformBadge platform={message.platform} />
      </span>
      <div className="min-w-0 flex-1 leading-snug">
        <b className={meta.text}>{message.author}</b>{" "}
        {message.badge && <span className="mr-1 rounded bg-[#2a2a3a] px-1.5 text-[11px]">{message.badge}</span>}
        <span className="text-zinc-300">{message.text}</span>
        {actions && (
          <div className="mt-2 hidden gap-2 group-hover:flex">
            {["Répondre", "Épingler", "Supprimer", "Timeout"].map((a) => (
              <button key={a} className="min-h-8 rounded-lg bg-[#2a2a3a] px-3 text-xs hover:bg-[#34344a]">
                {a}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
