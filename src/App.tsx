import { useState } from "react";
import Accounts from "./pages/Accounts";
import Dashboard from "./pages/Dashboard";
import Moderation from "./pages/Moderation";
import MultiChat from "./pages/MultiChat";
import StreamManager from "./pages/StreamManager";

const SECTIONS = [
  { id: "dashboard", label: "Tableau de bord", Component: Dashboard },
  { id: "chat", label: "Multi-chat", Component: MultiChat },
  { id: "moderation", label: "Modération", Component: Moderation },
  { id: "stream", label: "Stream manager", Component: StreamManager },
  { id: "accounts", label: "Comptes", Component: Accounts },
] as const;

export default function App() {
  const [current, setCurrent] = useState<(typeof SECTIONS)[number]["id"]>("dashboard");
  const Active = SECTIONS.find((s) => s.id === current)!.Component;

  return (
    <div className="flex h-screen bg-zinc-950 text-zinc-100">
      <nav className="flex w-56 flex-col gap-1 border-r border-zinc-800 p-3">
        <div className="mb-4 px-2 text-lg font-bold">Omnistream</div>
        {SECTIONS.map(({ id, label }) => (
          <button
            key={id}
            onClick={() => setCurrent(id)}
            className={`rounded px-3 py-2 text-left text-sm ${
              id === current ? "bg-violet-600" : "hover:bg-zinc-800"
            }`}
          >
            {label}
          </button>
        ))}
      </nav>
      <main className="flex-1 overflow-auto p-6">
        <Active />
      </main>
    </div>
  );
}
