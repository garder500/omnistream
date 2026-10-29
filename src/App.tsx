import { useState } from "react";
import { Logo } from "./components/Logo";
import Accounts from "./pages/Accounts";
import Dashboard from "./pages/Dashboard";
import Moderation from "./pages/Moderation";
import MultiChat from "./pages/MultiChat";
import StreamManager from "./pages/StreamManager";
import { FLAGGED } from "./lib/mock";

type SectionId = "dashboard" | "chat" | "moderation" | "stream" | "accounts";

const SECTIONS: { id: SectionId; label: string; icon: string; Component: (p: { go: (id: SectionId) => void }) => JSX.Element }[] = [
  { id: "dashboard", label: "Tableau de bord", icon: "M3 3h7v9H3zM14 3h7v5h-7zM14 12h7v9h-7zM3 16h7v5H3z", Component: Dashboard },
  { id: "chat", label: "Multi-chat", icon: "M4 5h16v11H9l-5 4z", Component: MultiChat },
  { id: "moderation", label: "Modération", icon: "M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z", Component: Moderation },
  { id: "stream", label: "Stream manager", icon: "M12 9a3 3 0 100 6 3 3 0 000-6zM6.3 6.3a8 8 0 000 11.4M17.7 6.3a8 8 0 010 11.4", Component: StreamManager },
  { id: "accounts", label: "Comptes", icon: "M12 4a4 4 0 100 8 4 4 0 000-8zM4 21c1-4 4-6 8-6s7 2 8 6", Component: Accounts },
];

export default function App() {
  const [current, setCurrent] = useState<SectionId>("dashboard");
  const Active = SECTIONS.find((s) => s.id === current)!.Component;

  return (
    <div className="flex h-screen bg-ink text-sm text-zinc-100">
      <nav className="flex w-58 shrink-0 flex-col gap-1 border-r border-[#1f1f2b] p-3.5">
        <div className="px-2 pb-5 pt-1">
          <Logo />
        </div>
        {SECTIONS.map(({ id, label, icon }) => (
          <button
            key={id}
            onClick={() => setCurrent(id)}
            aria-current={id === current ? "page" : undefined}
            className={`flex items-center gap-3 rounded-xl px-3 py-2.5 text-left ${
              id === current ? "bg-brand font-semibold text-white" : "text-[#b4b4c7] hover:bg-[#1a1a24]"
            }`}
          >
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d={icon} />
            </svg>
            {label}
            {id === "moderation" && (
              <span className="ml-auto rounded-full bg-amber-500 px-2 text-xs font-semibold text-[#1a1200]">{FLAGGED.length}</span>
            )}
          </button>
        ))}
      </nav>
      <main className="min-w-0 flex-1 overflow-auto p-7">
        <Active go={setCurrent} />
      </main>
    </div>
  );
}
