import { useEffect, useRef } from "react";
import { ChatMessage } from "./components/ChatMessage";
import { Logo } from "./components/Logo";
import { api } from "./lib/api";
import { StoreProvider, useStore } from "./lib/store";

/** Mode dock : petite fenêtre toujours au premier plan qui n'affiche que le chat unifié. */
export default function DockApp() {
  return (
    <StoreProvider>
      <Dock />
    </StoreProvider>
  );
}

function Dock() {
  const { messages, t } = useStore();
  const scroller = useRef<HTMLElement>(null);
  const stick = useRef(true);

  // Colle en bas sauf si l'utilisateur a remonté le chat pour relire.
  useEffect(() => {
    const el = scroller.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [messages.length]);

  return (
    <div className="flex h-screen flex-col bg-ink text-sm text-zinc-100">
      <header className="flex shrink-0 items-center justify-between gap-2 border-b border-line px-3 py-2">
        <Logo />
        <button
          onClick={() => api.toggleDock().catch(() => window.close())}
          className="min-h-9 rounded-lg bg-raised px-3 text-xs font-medium hover:bg-line-strong"
        >
          {t("dock.close")}
        </button>
      </header>
      <main
        ref={scroller}
        onScroll={(e) => {
          const el = e.currentTarget;
          stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
        }}
        className="min-h-0 flex-1 overflow-auto p-1.5" aria-label={t("dock.title")}>
        {messages.length === 0 && <p className="p-4 text-center text-muted">{t("dock.empty")}</p>}
        <div role="log" aria-live="polite" className="flex flex-col gap-0.5">
          {messages.map((m) => (
            <ChatMessage key={m.id} message={m} />
          ))}
        </div>
      </main>
    </div>
  );
}
