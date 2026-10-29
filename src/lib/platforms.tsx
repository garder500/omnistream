import type { Platform } from "./types";

export type PlatformKey = "twitch" | "youtube" | "kick" | "tiktok";

interface PlatformMeta {
  label: string;
  letter: string;
  /** Fond du badge (texte sombre dessus, contraste suffisant). */
  badge: string;
  /** Couleur de texte pour les pseudos. */
  text: string;
  /** TikTok n'a pas d'API officielle : lecture seule via un pseudo. */
  readOnly: boolean;
}

export const PLATFORMS: Record<PlatformKey, PlatformMeta> = {
  twitch: { label: "Twitch", letter: "T", badge: "bg-twitch text-[#120826]", text: "text-[#c9a8ff]", readOnly: false },
  youtube: { label: "YouTube", letter: "Y", badge: "bg-youtube text-[#1a0508]", text: "text-[#ff9c9c]", readOnly: false },
  kick: { label: "Kick", letter: "K", badge: "bg-kick text-[#0b1a04]", text: "text-[#8cf05b]", readOnly: false },
  tiktok: { label: "TikTok", letter: "Tk", badge: "bg-tiktok text-[#032a29]", text: "text-[#7ff7f2]", readOnly: true },
};

export const PLATFORM_KEYS = Object.keys(PLATFORMS) as PlatformKey[];

export function platformKey(p: Platform): PlatformKey {
  const key = typeof p === "string" ? p : p.other;
  return key in PLATFORMS ? (key as PlatformKey) : "twitch";
}

export function PlatformBadge({ platform, size = 22 }: { platform: PlatformKey; size?: number }) {
  const meta = PLATFORMS[platform];
  return (
    <span
      title={meta.label}
      style={{ width: size, height: size, fontSize: size * 0.5 }}
      className={`inline-flex shrink-0 items-center justify-center rounded-md font-bold ${meta.badge}`}
    >
      {meta.letter}
    </span>
  );
}
