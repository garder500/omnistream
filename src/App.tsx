import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CommandPalette, type Command } from "./components/CommandPalette";
import { Logo } from "./components/Logo";
import { NavItem } from "./components/ui";
import Accounts from "./pages/Accounts";
import Automations from "./pages/Automations";
import Commands from "./pages/Commands";
import Dashboard from "./pages/Dashboard";
import History from "./pages/History";
import Moderation from "./pages/Moderation";
import MultiChat from "./pages/MultiChat";
import Overlays from "./pages/Overlays";
import StreamManager from "./pages/StreamManager";
import { api } from "./lib/api";
import type { Key } from "./lib/i18n";
import { AiProvider } from "./lib/ai";
import { flaggedQueue, StoreProvider, useStore } from "./lib/store";

type SectionId = "dashboard" | "chat" | "moderation" | "automations" | "commands" | "stream" | "history" | "overlays" | "accounts";

const SECTIONS: { id: SectionId; labelKey: Key; icon: string; Component: (p: { go: (id: SectionId) => void }) => JSX.Element }[] = [
  { id: "dashboard", labelKey: "nav.dashboard", icon: "M3 3h7v9H3zM14 3h7v5h-7zM14 12h7v9h-7zM3 16h7v5H3z", Component: Dashboard },
  { id: "chat", labelKey: "nav.chat", icon: "M4 5h16v11H9l-5 4z", Component: MultiChat },
  { id: "moderation", labelKey: "nav.moderation", icon: "M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z", Component: Moderation },
  { id: "automations", labelKey: "nav.automations", icon: "M13 2L4 14h7l-1 8 9-12h-7z", Component: Automations },
  { id: "commands", labelKey: "nav.commands", icon: "M4 17l6-6-6-6M12 19h8", Component: Commands },
  { id: "stream", labelKey: "nav.stream", icon: "M12 9a3 3 0 100 6 3 3 0 000-6zM6.3 6.3a8 8 0 000 11.4M17.7 6.3a8 8 0 010 11.4", Component: StreamManager },
  { id: "history", labelKey: "nav.history", icon: "M3 12a9 9 0 109-9 9 9 0 00-6.4 2.6L3 8M3 3v5h5M12 7v5l3 2", Component: History },
  { id: "overlays", labelKey: "nav.overlays", icon: "M3 5h18v14H3zM7 9h6M7 13h10", Component: Overlays },
  { id: "accounts", labelKey: "nav.accounts", icon: "M12 4a4 4 0 100 8 4 4 0 000-8zM4 21c1-4 4-6 8-6s7 2 8 6", Component: Accounts },
];

export default function App() {
  return (
    <StoreProvider>
      <AiProvider>
        <Shell />
      </AiProvider>
    </StoreProvider>
  );
}

function Shell() {
  const { messages, toasts, t, prefs, setPrefs, notify } = useStore();
  const [current, setCurrent] = useState<SectionId>("dashboard");
  const [paletteOpen, setPaletteOpen] = useState(false);
  const main = useRef<HTMLElement>(null);
  const Active = SECTIONS.find((s) => s.id === current)!.Component;

  const go = useCallback((id: SectionId) => {
    setCurrent(id);
    // Le focus suit la navigation (lecteurs d'écran, clavier).
    requestAnimationFrame(() => main.current?.focus({ preventScroll: true }));
  }, []);

  const toggleDock = useCallback(() => {
    api.toggleDock().catch((e) => notify(String(e), true));
  }, [notify]);

  // Ctrl/Cmd+K : palette ; Alt+1..5 : pages.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen((o) => !o);
      } else if (e.altKey && !e.ctrlKey && !e.metaKey && /^(Digit|Numpad)[1-5]$/.test(e.code)) {
        // `code` et non `key` : fiable sur AZERTY et macOS.
        e.preventDefault();
        go(SECTIONS[Number(e.code.slice(-1)) - 1].id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [go]);

  // Raccourcis globaux enregistrés côté Rust (fonctionnent même fenêtre non focalisée).
  useEffect(() => {
    let cancelled = false;
    const un = listen("shortcut://palette", () => !cancelled && setPaletteOpen(true));
    return () => {
      cancelled = true;
      un.then((fn) => fn()).catch(() => {});
    };
  }, []);

  const commands = useMemo<Command[]>(
    () => [
      ...SECTIONS.map((s, i) => ({
        id: `go-${s.id}`,
        group: "pages" as const,
        label: t("palette.goto", { page: t(s.labelKey) }),
        shortcut: `Alt+${i + 1}`,
        run: () => go(s.id),
      })),
      {
        id: "lang",
        group: "actions",
        label: t("palette.toggleLang"),
        run: () => setPrefs({ lang: prefs.lang === "fr" ? "en" : "fr" }),
      },
      {
        id: "contrast",
        group: "actions",
        label: t("palette.toggleContrast"),
        run: () => {
          setPrefs({ highContrast: !prefs.highContrast });
          notify(t(prefs.highContrast ? "settings.contrastOff" : "settings.contrastOn"));
        },
      },
      {
        id: "notifs",
        group: "actions",
        label: t("palette.toggleNotifs"),
        run: () => {
          setPrefs({ notifications: !prefs.notifications });
          notify(t(prefs.notifications ? "settings.notifsOff" : "settings.notifsOn"));
        },
      },
      { id: "dock", group: "actions", label: t("palette.toggleDock"), shortcut: "Ctrl+Shift+D", run: toggleDock },
    ],
    [t, go, setPrefs, prefs, notify, toggleDock],
  );

  const pill = (active: boolean) =>
    `min-h-9 flex-1 rounded-lg px-2 text-xs font-semibold transition ${active ? "bg-brand text-white" : "bg-field text-muted hover:text-zinc-100"}`;

  return (
    <div className="flex h-screen bg-ink text-sm text-zinc-100">
      <a href="#main" className="sr-only focus:not-sr-only focus:absolute focus:left-3 focus:top-3 focus:z-50 focus:rounded-control focus:bg-brand focus:px-4 focus:py-2">
        {t("nav.skip")}
      </a>
      <nav aria-label={t("nav.main")} className="flex w-60 shrink-0 flex-col gap-1.5 border-r border-line p-4">
        <div className="px-2 pb-5 pt-1">
          <Logo />
        </div>
        {SECTIONS.map(({ id, labelKey, icon }) => (
          <NavItem
            key={id}
            active={id === current}
            icon={icon}
            label={t(labelKey)}
            count={id === "moderation" ? flaggedQueue(messages).length || undefined : undefined}
            onClick={() => go(id)}
          />
        ))}
        <div className="mt-auto flex flex-col gap-2 border-t border-line pt-3">
          <button onClick={() => setPaletteOpen(true)} className="flex min-h-9 items-center justify-between rounded-lg bg-field px-3 text-xs text-muted hover:text-zinc-100">
            {t("settings.palette")}
            <kbd className="rounded border border-line-strong px-1.5">Ctrl K</kbd>
          </button>
          <div role="group" aria-label={t("settings.language")} className="flex gap-1.5">
            {(["fr", "en"] as const).map((l) => (
              <button key={l} aria-pressed={prefs.lang === l} lang={l} onClick={() => setPrefs({ lang: l })} className={pill(prefs.lang === l)}>
                {l.toUpperCase()}
              </button>
            ))}
          </div>
          <div className="flex gap-1.5">
            <button aria-pressed={prefs.highContrast} onClick={() => setPrefs({ highContrast: !prefs.highContrast })} className={pill(prefs.highContrast)}>
              {t("settings.contrast")}
            </button>
            <button onClick={toggleDock} className={pill(false)}>
              {t("settings.dock")}
            </button>
          </div>
        </div>
      </nav>
      <main id="main" ref={main} tabIndex={-1} className="min-w-0 flex-1 overflow-auto p-7 outline-none">
        <Active go={go} />
      </main>
      <div className="pointer-events-none fixed bottom-5 right-5 flex w-96 flex-col gap-2" role="status" aria-live="polite">
        {toasts.map((x) => (
          <div key={x.id} className={`rounded-control border px-4 py-3 shadow-xl ${x.error ? "border-danger-line bg-danger-soft text-danger-text" : "border-line bg-panel"}`}>
            {x.text}
          </div>
        ))}
      </div>
      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} commands={commands} />
    </div>
  );
}
