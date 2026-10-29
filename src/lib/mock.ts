// Données d'exemple pour les écrans, en attendant le branchement sur les commandes Tauri.
import type { PlatformKey } from "./platforms";

export interface MockMessage {
  id: string;
  platform: PlatformKey;
  author: string;
  text: string;
  badge?: string;
  flagged?: string;
}

export const MESSAGES: MockMessage[] = [
  { id: "1", platform: "twitch", author: "lunaplays", badge: "Abo", text: "GG pour le boss !! on l'a enfin eu" },
  { id: "2", platform: "youtube", author: "Mathieu_R", text: "Le son est parfait aujourd'hui" },
  { id: "3", platform: "kick", author: "Kaizen", badge: "Modo", text: "on est dans la place depuis Kick" },
  { id: "4", platform: "tiktok", author: "mia.clips", text: "je viens du TikTok, trop bien le live" },
  { id: "5", platform: "twitch", author: "pixel_fox", text: "tu peux remettre la musique d'hier ?" },
  { id: "6", platform: "youtube", author: "Sarah.b", text: "première fois ici, super ambiance" },
  { id: "7", platform: "kick", author: "bob_le_bricoleur", text: "quelqu'un connaît le pseudo du compositeur ?" },
  { id: "8", platform: "youtube", author: "Théo_75", text: "on peut avoir un replay du dernier combat ?" },
  { id: "9", platform: "twitch", author: "neon_kat", text: "+1 pour le replay" },
];

export interface MockEvent {
  id: string;
  platform: PlatformKey;
  who: string;
  what: string;
  detail?: string;
  age: string;
  highlight?: "donation" | "sub";
}

export const EVENTS: MockEvent[] = [
  { id: "e1", platform: "twitch", who: "Léa_G", what: "a donné 10 €", detail: "« Pour la musique ! »", age: "2 min", highlight: "donation" },
  { id: "e2", platform: "twitch", who: "pixel_fox", what: "s'abonne · 14 mois", age: "6 min", highlight: "sub" },
  { id: "e3", platform: "youtube", who: "Sarah.b", what: "est devenue membre", age: "11 min" },
  { id: "e4", platform: "kick", who: "Kaizen", what: "vous suit", age: "15 min" },
  { id: "e5", platform: "twitch", who: "SpeedyGonz", what: "raid avec 85 viewers", age: "38 min" },
];

export const VIEWERS: Record<PlatformKey, number> = { twitch: 2104, youtube: 1987, kick: 721, tiktok: 342 };

export interface FlaggedMessage {
  id: string;
  platform: PlatformKey;
  author: string;
  text: string;
  reason: string;
  age: string;
}

export const FLAGGED: FlaggedMessage[] = [
  { id: "f1", platform: "kick", author: "xX_spam_Xx", text: "achetez des abonnés sur bit.ly/xxxx pas cher", reason: "Lien suspect", age: "20 s" },
  { id: "f2", platform: "twitch", author: "troll_2000", text: "t'es nul arrête le stream […]", reason: "Insulte", age: "1 min" },
  { id: "f3", platform: "youtube", author: "Anon_8841", text: "REGARDEZ MA CHAINE REGARDEZ MA CHAINE", reason: "Majuscules", age: "3 min" },
];
