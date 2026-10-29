import type { ReactNode } from "react";

export function Page({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <section>
      <h1 className="mb-4 text-2xl font-semibold">{title}</h1>
      {children ?? <p className="text-zinc-400">À venir.</p>}
    </section>
  );
}
