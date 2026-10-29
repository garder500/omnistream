import { useEffect, useId, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode, type SelectHTMLAttributes } from "react";
import { PLATFORMS, PlatformBadge, type PlatformKey } from "../lib/platforms";

export function Card({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <div className={`rounded-2xl border border-line bg-panel ${className}`}>{children}</div>;
}

type Variant = "primary" | "neutral" | "danger" | "warn" | "ghost";

const VARIANTS: Record<Variant, string> = {
  primary: "bg-brand text-white hover:bg-violet-500",
  neutral: "bg-[#2a2a3a] text-zinc-100 hover:bg-[#34344a]",
  danger: "bg-youtube text-[#1a0508] hover:brightness-110",
  warn: "bg-[#3a2d12] text-amber-300 hover:brightness-125",
  ghost: "border border-[#5a2229] text-[#ff8a8a] hover:bg-[#2a1216]",
};

export function Button({
  variant = "neutral",
  className = "",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  return (
    <button
      {...props}
      className={`min-h-11 rounded-xl px-5 py-2.5 text-sm font-semibold transition disabled:opacity-50 ${VARIANTS[variant]} ${className}`}
    />
  );
}

export function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={`relative h-7 w-12 shrink-0 rounded-full transition ${checked ? "bg-brand" : "bg-line-strong"}`}
    >
      <span className={`absolute top-1 h-5 w-5 rounded-full bg-white transition-all ${checked ? "left-6" : "left-1"}`} />
    </button>
  );
}

export function PageHeader({ title, subtitle, actions }: { title: string; subtitle?: string; actions?: ReactNode }) {
  return (
    <header className="mb-5 flex flex-wrap items-center justify-between gap-3">
      <div>
        <h1 className="text-3xl font-bold tracking-tight">{title}</h1>
        {subtitle && <p className="mt-1 text-sm text-muted">{subtitle}</p>}
      </div>
      {actions && <div className="flex items-center gap-3">{actions}</div>}
    </header>
  );
}

const FIELD =
  "min-h-11 w-full rounded-xl border border-line-strong bg-field px-4 py-2.5 text-[15px] text-zinc-100 placeholder:text-muted/70 outline-none transition hover:bg-field-hover focus:border-brand focus:ring-4 focus:ring-brand/20 disabled:opacity-50";

/** Étiquette + champ + aide/erreur, avec un espacement régulier. */
export function Field({ label, hint, error, children }: { label: string; hint?: string; error?: string; children: ReactNode }) {
  return (
    <label className="flex flex-col gap-2">
      <span className="text-sm font-medium text-zinc-300">{label}</span>
      {children}
      {(error || hint) && <span className={`text-xs ${error ? "text-youtube" : "text-muted"}`}>{error ?? hint}</span>}
    </label>
  );
}

export function Input({ className = "", ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return <input {...props} className={`${FIELD} ${className}`} />;
}

const SELECT_ARROW = `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16' fill='none' stroke='%239a9ab0' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M4 6l4 4 4-4'/%3E%3C/svg%3E")`;

export function Select({ className = "", children, ...props }: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select
      {...props}
      style={{ backgroundImage: SELECT_ARROW, ...props.style }}
      className={`${FIELD} appearance-none bg-[length:1rem] bg-[right_1rem_center] bg-no-repeat pr-10 ${className}`}
    >
      {children}
    </select>
  );
}

/** Case à cocher personnalisée, zone cliquable étendue à toute la ligne. */
export function Checkbox({ checked, onChange, children }: { checked: boolean; onChange: (v: boolean) => void; children: ReactNode }) {
  return (
    <label className="flex min-h-11 cursor-pointer items-center gap-3 rounded-xl px-3 hover:bg-field-hover">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} className="peer sr-only" />
      <span
        aria-hidden="true"
        className="grid h-5 w-5 shrink-0 place-items-center rounded-md border-2 border-line-strong bg-field text-transparent transition peer-checked:border-brand peer-checked:bg-brand peer-checked:text-white peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-brand-ring"
      >
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M2.5 6.5l2.5 2.5 4.5-5.5" />
        </svg>
      </span>
      <span className="text-[15px]">{children}</span>
    </label>
  );
}

/** Pastille sélectionnable d'une plateforme (badge + nom), pour « Appliquer à », « Envoyer sur »… */
export function PlatformChip({ platform, selected, onToggle }: { platform: PlatformKey; selected: boolean; onToggle: () => void }) {
  return (
    <button
      type="button"
      aria-pressed={selected}
      onClick={onToggle}
      className={`flex min-h-11 items-center gap-2.5 rounded-xl border px-3.5 py-2 text-[15px] font-medium transition ${
        selected ? "border-brand bg-brand-soft text-white" : "border-line-strong bg-field text-muted hover:bg-field-hover hover:text-zinc-100"
      }`}
    >
      <span className={selected ? "" : "opacity-50"}>
        <PlatformBadge platform={platform} size={24} />
      </span>
      {PLATFORMS[platform].label}
    </button>
  );
}

/** Groupe de PlatformChip avec légende, espacé pour respirer. */
export function PlatformChipGroup({
  legend,
  platforms,
  selected,
  onToggle,
}: {
  legend: string;
  platforms: PlatformKey[];
  selected: PlatformKey[];
  onToggle: (p: PlatformKey) => void;
}) {
  return (
    <fieldset className="flex flex-col gap-3">
      <legend className="mb-3 text-sm font-medium text-zinc-300">{legend}</legend>
      <div className="flex flex-wrap gap-3">
        {platforms.map((p) => (
          <PlatformChip key={p} platform={p} selected={selected.includes(p)} onToggle={() => onToggle(p)} />
        ))}
      </div>
    </fieldset>
  );
}

const BADGE_TONES = {
  brand: "bg-brand-soft text-violet-200",
  success: "bg-[#12301c] text-green-300",
  warn: "bg-[#3a2d12] text-amber-300",
  danger: "bg-[#3a1217] text-red-300",
  neutral: "bg-line-strong text-zinc-200",
};

export function Badge({ tone = "neutral", children }: { tone?: keyof typeof BADGE_TONES; children: ReactNode }) {
  return <span className={`inline-flex w-fit items-center rounded-full px-3 py-1 text-xs font-semibold ${BADGE_TONES[tone]}`}>{children}</span>;
}

export function Tabs<T extends string>({ tabs, value, onChange }: { tabs: { id: T; label: string }[]; value: T; onChange: (id: T) => void }) {
  return (
    <div role="tablist" className="inline-flex gap-1 rounded-xl border border-line bg-field p-1">
      {tabs.map((t) => (
        <button
          key={t.id}
          role="tab"
          aria-selected={t.id === value}
          onClick={() => onChange(t.id)}
          className={`min-h-10 rounded-lg px-4 text-sm font-medium transition ${t.id === value ? "bg-brand text-white" : "text-muted hover:text-zinc-100"}`}
        >
          {t.label}
        </button>
      ))}
    </div>
  );
}

export function Modal({ open, title, onClose, children }: { open: boolean; title: string; onClose: () => void; children: ReactNode }) {
  const titleId = useId();
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/60 p-6" onClick={onClose}>
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onClick={(e) => e.stopPropagation()}
        className="w-full max-w-md rounded-2xl border border-line bg-panel p-6 shadow-2xl"
      >
        <h2 id={titleId} className="mb-5 text-xl font-bold">
          {title}
        </h2>
        {children}
      </div>
    </div>
  );
}
