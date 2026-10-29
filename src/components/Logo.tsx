/** Logo « Orbite » : un anneau, un satellite par plateforme. */
export function LogoMark({ size = 30 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" aria-hidden="true">
      <circle cx="32" cy="32" r="21" fill="none" stroke="#7c3aed" strokeWidth="5" />
      <circle cx="32" cy="11" r="6.5" fill="#a970ff" stroke="#0b0b10" strokeWidth="2.5" />
      <circle cx="50.2" cy="42.5" r="6.5" fill="#ff5c5c" stroke="#0b0b10" strokeWidth="2.5" />
      <circle cx="13.8" cy="42.5" r="6.5" fill="#53fc18" stroke="#0b0b10" strokeWidth="2.5" />
      <circle cx="32" cy="32" r="4.5" fill="#fff" />
    </svg>
  );
}

export function Logo() {
  return (
    <div className="flex items-center gap-2.5">
      <LogoMark />
      <span className="text-xl font-bold tracking-tight">Omnistream</span>
    </div>
  );
}
