import { platformKey, type PlatformKey } from "./platforms";
import type { Channel, StreamInfo } from "./types";

/** En dessous, trop peu de chatteurs pour tirer un taux de recoupement. */
export const MIN_CHATTERS_FOR_ESTIMATE = 10;

export interface ViewerEstimate {
  /** Somme brute des spectateurs de toutes les chaînes en direct. */
  total: number;
  /** Estimation de spectateurs uniques, entre `low` et `total`. */
  unique: number;
  /** Borne basse : tous les spectateurs des plus petites plateformes sont des doublons. */
  low: number;
  /** `false` quand l'estimation n'est pas fondée (une seule plateforme, ou trop peu de chat). */
  reliable: boolean;
  /** Part des chatteurs vus sur plusieurs plateformes, 0–1. */
  overlapRate: number;
}

export interface ChatterOverlap {
  chatters: number;
  shared: number;
}

export function viewersByPlatform(infos: StreamInfo[], channels: Channel[]): Partial<Record<PlatformKey, number>> {
  const out: Partial<Record<PlatformKey, number>> = {};
  for (const info of infos) {
    const channel = channels.find((c) => c.id === info.channel_id);
    if (!channel) continue;
    const key = platformKey(channel.platform);
    out[key] = (out[key] ?? 0) + info.viewers;
  }
  return out;
}

/**
 * Estime les spectateurs uniques. Hypothèse : la part de chatteurs présents sur
 * plusieurs plateformes (même pseudo) vaut pour l'ensemble des spectateurs. Les
 * doublons ne peuvent concerner que ceux hors de la plus grande plateforme, d'où
 * `unique = total - (total - max) × taux`. C'est une approximation (homonymes,
 * pseudos différents, spectateurs qui ne parlent pas) : à afficher comme telle.
 */
export function estimateUnique(byPlatform: Partial<Record<PlatformKey, number>>, overlap: ChatterOverlap): ViewerEstimate {
  const counts = Object.values(byPlatform).filter((n): n is number => typeof n === "number" && n > 0);
  const total = counts.reduce((a, b) => a + b, 0);
  const max = counts.length ? Math.max(...counts) : 0;
  const reliable = counts.length > 1 && overlap.chatters >= MIN_CHATTERS_FOR_ESTIMATE;
  const overlapRate = overlap.chatters > 0 ? Math.min(1, overlap.shared / overlap.chatters) : 0;
  const unique = reliable ? Math.round(total - (total - max) * overlapRate) : total;
  return { total, unique, low: max, reliable, overlapRate };
}
