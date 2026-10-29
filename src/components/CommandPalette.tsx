import { useEffect, useId, useMemo, useRef, useState } from "react";
import { useStore } from "../lib/store";

export interface Command {
  id: string;
  label: string;
  group: "pages" | "actions";
  /** Raccourci affiché à droite (indicatif). */
  shortcut?: string;
  run: () => void;
}

const fold = (s: string) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();

/** Correspondance par sous-chaîne, sinon par sous-séquence (« tdb » → « Tableau de bord »). */
function score(label: string, query: string): number {
  const l = fold(label);
  const q = fold(query).trim();
  if (!q) return 1;
  const at = l.indexOf(q);
  if (at >= 0) return 100 - at;
  let i = 0;
  for (const ch of l) if (ch === q[i]) i++;
  return i === q.length ? 1 : 0;
}

export function CommandPalette({ open, onClose, commands }: { open: boolean; onClose: () => void; commands: Command[] }) {
  const { t } = useStore();
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const previous = useRef<HTMLElement | null>(null);
  const listId = useId();
  const titleId = useId();

  // Ordre = ordre d'affichage (pages puis actions), pour que flèches et surbrillance coïncident.
  const results = useMemo(() => {
    const scored = commands.map((c) => ({ c, s: score(c.label, query) })).filter((x) => x.s > 0);
    return (["pages", "actions"] as const).flatMap((g) =>
      scored
        .filter((x) => x.c.group === g)
        .sort((a, b) => b.s - a.s)
        .map((x) => x.c),
    );
  }, [commands, query]);

  useEffect(() => {
    if (!open) return;
    previous.current = document.activeElement as HTMLElement | null;
    setQuery("");
    setActive(0);
    input.current?.focus();
    return () => previous.current?.focus?.();
  }, [open]);

  useEffect(() => setActive(0), [query]);

  if (!open) return null;

  const run = (c: Command | undefined) => {
    if (!c) return;
    onClose();
    c.run();
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (results.length) setActive((a) => (a + (e.key === "ArrowDown" ? 1 : -1) + results.length) % results.length);
    } else if (e.key === "Enter") {
      e.preventDefault();
      run(results[active]);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onClose();
    } else if (e.key === "Tab") {
      e.preventDefault(); // le focus reste dans le champ : la liste se pilote aux flèches
    }
  };

  const groups = (["pages", "actions"] as const).map((g) => ({ g, items: results.filter((c) => c.group === g) })).filter((x) => x.items.length);

  return (
    <div className="fixed inset-0 z-50 grid place-items-start justify-center bg-black/60 p-6 pt-[14vh]" onMouseDown={onClose}>
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        className="w-full max-w-xl overflow-hidden rounded-card border border-line-strong bg-panel shadow-pop"
      >
        <h2 id={titleId} className="sr-only">
          {t("palette.title")}
        </h2>
        <input
          ref={input}
          role="combobox"
          aria-expanded="true"
          aria-controls={listId}
          aria-activedescendant={results[active] ? `${listId}-${results[active].id}` : undefined}
          aria-autocomplete="list"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={t("palette.placeholder")}
          className="w-full border-b border-line bg-transparent px-5 py-4 text-base text-zinc-100 outline-none placeholder:text-muted/70"
        />
        <div id={listId} role="listbox" aria-label={t("palette.title")} className="max-h-80 overflow-auto p-2">
          {results.length === 0 && <p className="p-6 text-center text-muted">{t("palette.empty")}</p>}
          {groups.map(({ g, items }) => (
            <div key={g} role="group" aria-label={t(g === "pages" ? "palette.group.pages" : "palette.group.actions")}>
              <div aria-hidden="true" className="px-3 pb-1 pt-2 text-xs font-semibold uppercase tracking-wide text-muted">
                {t(g === "pages" ? "palette.group.pages" : "palette.group.actions")}
              </div>
              {items.map((c) => {
                const selected = results[active]?.id === c.id;
                return (
                  <div
                    key={c.id}
                    id={`${listId}-${c.id}`}
                    role="option"
                    aria-selected={selected}
                    ref={(el) => selected && el?.scrollIntoView({ block: "nearest" })}
                    onMouseMove={() => setActive(results.indexOf(c))}
                    onClick={() => run(c)}
                    className={`flex min-h-11 cursor-pointer items-center justify-between gap-3 rounded-control px-3.5 ${selected ? "bg-brand-soft text-white ring-1 ring-brand-line" : "text-soft"}`}
                  >
                    <span>{c.label}</span>
                    {c.shortcut && <kbd className="rounded-md border border-line-strong bg-field px-2 py-0.5 text-xs text-muted">{c.shortcut}</kbd>}
                  </div>
                );
              })}
            </div>
          ))}
        </div>
        <div className="border-t border-line px-5 py-2.5 text-xs text-muted">{t("palette.hint")}</div>
      </div>
    </div>
  );
}
