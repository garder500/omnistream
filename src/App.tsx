import { useState } from "react";
import { Logo } from "./components/Logo";
import { NavItem } from "./components/ui";
import Accounts from "./pages/Accounts";
import Automations from "./pages/Automations";
import Commands from "./pages/Commands";
import Dashboard from "./pages/Dashboard";
import History from "./pages/History";
import Moderation from "./pages/Moderation";
import MultiChat from "./pages/MultiChat";
import StreamManager from "./pages/StreamManager";
import { flaggedQueue, StoreProvider, useStore } from "./lib/store";

type SectionId = "dashboard" | "chat" | "moderation" | "automations" | "commands" | "stream" | "history" | "accounts";

const SECTIONS: { id: SectionId; label: string; icon: string; Component: (p: { go: (id: SectionId) => void }) => JSX.Element }[] = [
  { id: "dashboard", label: "Tableau de bord", icon: "M3 3h7v9H3zM14 3h7v5h-7zM14 12h7v9h-7zM3 16h7v5H3z", Component: Dashboard },
  { id: "chat", label: "Multi-chat", icon: "M4 5h16v11H9l-5 4z", Component: MultiChat },
  { id: "moderation", label: "Modération", icon: "M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z", Component: Moderation },
  { id: "automations", label: "Automations", icon: "M13 2L4 14h7l-1 8 9-12h-7z", Component: Automations },
  { id: "commands", label: "Commandes", icon: "M4 17l6-6-6-6M12 19h8", Component: Commands },
  { id: "stream", label: "Stream manager", icon: "M12 9a3 3 0 100 6 3 3 0 000-6zM6.3 6.3a8 8 0 000 11.4M17.7 6.3a8 8 0 010 11.4", Component: StreamManager },
  { id: "history", label: "Historique", icon: "M3 12a9 9 0 109-9 9 9 0 00-6.4 2.6L3 8M3 3v5h5M12 7v5l3 2", Component: History },
  { id: "accounts", label: "Comptes", icon: "M12 4a4 4 0 100 8 4 4 0 000-8zM4 21c1-4 4-6 8-6s7 2 8 6", Component: Accounts },
];

export default function App() {
  return (
    <StoreProvider>
      <Shell />
    </StoreProvider>
  );
}

function Shell() {
  const { messages, toasts } = useStore();
  const [current, setCurrent] = useState<SectionId>("dashboard");
  const Active = SECTIONS.find((s) => s.id === current)!.Component;

  return (
    <div className="flex h-screen bg-ink text-sm text-zinc-100">
      <nav className="flex w-60 shrink-0 flex-col gap-1.5 border-r border-line p-4">
        <div className="px-2 pb-5 pt-1">
          <Logo />
        </div>
        {SECTIONS.map(({ id, label, icon }) => (
          <NavItem
            key={id}
            active={id === current}
            icon={icon}
            label={label}
            count={id === "moderation" ? flaggedQueue(messages).length || undefined : undefined}
            onClick={() => setCurrent(id)}
          />
        ))}
      </nav>
      <main className="min-w-0 flex-1 overflow-auto p-7">
        <Active go={setCurrent} />
      </main>
      <div className="pointer-events-none fixed bottom-5 right-5 flex w-96 flex-col gap-2" role="status">
        {toasts.map((t) => (
          <div key={t.id} className={`rounded-control border px-4 py-3 shadow-xl ${t.error ? "border-danger-line bg-danger-soft text-danger-text" : "border-line bg-panel"}`}>
            {t.text}
          </div>
        ))}
      </div>
    </div>
  );
}
