// Internationalisation légère : dictionnaires fr/en, interpolation `{nom}` et pluriels `clé_one` / `clé_other`.
export type Lang = "fr" | "en";
export const LANGS: Lang[] = ["fr", "en"];

const fr = {
  "nav.dashboard": "Tableau de bord",
  "nav.chat": "Multi-chat",
  "nav.moderation": "Modération",
  "nav.stream": "Stream manager",
  "nav.accounts": "Comptes",
  "nav.automations": "Automations",
  "nav.commands": "Commandes",
  "nav.history": "Historique",
  "nav.main": "Navigation principale",
  "nav.skip": "Aller au contenu",

  "common.live": "EN DIRECT",
  "common.soon": "Bientôt",
  "common.mod": "Modo",
  "common.sub": "Abo",
  "common.deleted": "supprimé",
  "common.delete": "Supprimer",
  "common.timeout10": "Timeout 10 min",
  "common.ban": "Bannir",
  "common.close": "Fermer",
  "common.messageDeleted": "Message supprimé",
  "common.banned": "Banni",

  "dash.title": "Tableau de bord",
  "dash.live_one": "Vous êtes en direct sur {n} chaîne",
  "dash.live_other": "Vous êtes en direct sur {n} chaînes",
  "dash.offline": "Aucun live en cours sur vos chaînes",
  "dash.viewers": "Spectateurs cumulés",
  "dash.viewersHintLive": "Mis à jour toutes les 30 s",
  "dash.viewersHintOff": "Aucun live détecté",
  "dash.uniquesEstimate": "≈ {n} uniques (estimation)",
  "dash.overlapTitle": "Estimation du dédoublonnage : {pct} % des chatteurs sont sur plusieurs plateformes. Fourchette {low} à {high}.",
  "dash.msgPerMin": "Messages / minute",
  "dash.merged_one": "{n} chaîne fusionnée",
  "dash.merged_other": "{n} chaînes fusionnées",
  "dash.connected": "Chats connectés",
  "dash.withAccount": "Compte Twitch connecté",
  "dash.readOnly": "Lecture seule : aucun compte",
  "dash.toModerate": "À modérer",
  "dash.openQueue": "Ouvrir la file →",
  "dash.comingSoon": "Bientôt disponible",
  "dash.noChannel": "Aucune chaîne suivie",
  "dash.viewersUnit": "spectateurs",
  "dash.add": "Ajouter →",
  "dash.unifiedChat": "Chat unifié",
  "dash.expand": "Agrandir →",
  "dash.noMessages": "Aucun message pour l'instant.",

  "palette.title": "Palette de commandes",
  "palette.placeholder": "Aller à une page ou lancer une action…",
  "palette.empty": "Aucun résultat",
  "palette.hint": "↑↓ naviguer · Entrée valider · Échap fermer",
  "palette.goto": "Aller à {page}",
  "palette.group.pages": "Pages",
  "palette.group.actions": "Actions",
  "palette.toggleLang": "Langue : passer en English",
  "palette.toggleContrast": "Basculer le contraste élevé",
  "palette.toggleNotifs": "Activer / désactiver les notifications",
  "palette.toggleDock": "Ouvrir / fermer le mode dock",

  "settings.language": "Langue",
  "settings.contrast": "Contraste élevé",
  "settings.contrastOn": "Contraste élevé activé",
  "settings.contrastOff": "Contraste élevé désactivé",
  "settings.notifsOn": "Notifications système activées",
  "settings.notifsOff": "Notifications système désactivées",
  "settings.palette": "Palette (Ctrl+K)",
  "settings.dock": "Mode dock",

  "dock.title": "Chat · dock",
  "dock.empty": "En attente de messages…",
  "dock.close": "Fermer le dock",

  "notif.flagged": "Message à modérer",
  "notif.sub": "{name} s'abonne",
  "notif.gift": "{name} offre un abonnement",
  "notif.raid": "{name} lance un raid ({viewers} viewers)",
} as const;

type AllKeys = keyof typeof fr;
type Plural<K> = K extends `${infer B}_one` ? B : never;
/** Clés simples ou base d'une clé pluralisée (`dash.live` → `dash.live_one` / `_other`). */
export type Key = AllKeys | Plural<AllKeys>;

const en: Record<AllKeys, string> = {
  "nav.dashboard": "Dashboard",
  "nav.chat": "Multi-chat",
  "nav.moderation": "Moderation",
  "nav.stream": "Stream manager",
  "nav.accounts": "Accounts",
  "nav.automations": "Automations",
  "nav.commands": "Commands",
  "nav.history": "History",
  "nav.main": "Main navigation",
  "nav.skip": "Skip to content",

  "common.live": "LIVE",
  "common.soon": "Soon",
  "common.mod": "Mod",
  "common.sub": "Sub",
  "common.deleted": "deleted",
  "common.delete": "Delete",
  "common.timeout10": "Timeout 10 min",
  "common.ban": "Ban",
  "common.close": "Close",
  "common.messageDeleted": "Message deleted",
  "common.banned": "Banned",

  "dash.title": "Dashboard",
  "dash.live_one": "You are live on {n} channel",
  "dash.live_other": "You are live on {n} channels",
  "dash.offline": "No live stream on your channels",
  "dash.viewers": "Total viewers",
  "dash.viewersHintLive": "Refreshed every 30 s",
  "dash.viewersHintOff": "No live detected",
  "dash.uniquesEstimate": "≈ {n} unique (estimate)",
  "dash.overlapTitle": "Deduplication estimate: {pct}% of chatters are on several platforms. Range {low} to {high}.",
  "dash.msgPerMin": "Messages / minute",
  "dash.merged_one": "{n} channel merged",
  "dash.merged_other": "{n} channels merged",
  "dash.connected": "Connected chats",
  "dash.withAccount": "Twitch account connected",
  "dash.readOnly": "Read-only: no account",
  "dash.toModerate": "To moderate",
  "dash.openQueue": "Open the queue →",
  "dash.comingSoon": "Coming soon",
  "dash.noChannel": "No channel followed",
  "dash.viewersUnit": "viewers",
  "dash.add": "Add →",
  "dash.unifiedChat": "Unified chat",
  "dash.expand": "Expand →",
  "dash.noMessages": "No messages yet.",

  "palette.title": "Command palette",
  "palette.placeholder": "Go to a page or run an action…",
  "palette.empty": "No results",
  "palette.hint": "↑↓ navigate · Enter select · Esc close",
  "palette.goto": "Go to {page}",
  "palette.group.pages": "Pages",
  "palette.group.actions": "Actions",
  "palette.toggleLang": "Language: switch to Français",
  "palette.toggleContrast": "Toggle high contrast",
  "palette.toggleNotifs": "Enable / disable notifications",
  "palette.toggleDock": "Open / close dock mode",

  "settings.language": "Language",
  "settings.contrast": "High contrast",
  "settings.contrastOn": "High contrast enabled",
  "settings.contrastOff": "High contrast disabled",
  "settings.notifsOn": "System notifications enabled",
  "settings.notifsOff": "System notifications disabled",
  "settings.palette": "Palette (Ctrl+K)",
  "settings.dock": "Dock mode",

  "dock.title": "Chat · dock",
  "dock.empty": "Waiting for messages…",
  "dock.close": "Close dock",

  "notif.flagged": "Message to moderate",
  "notif.sub": "{name} subscribed",
  "notif.gift": "{name} gifted a subscription",
  "notif.raid": "{name} is raiding ({viewers} viewers)",
};

const DICTS: Record<Lang, Record<AllKeys, string>> = { fr, en };

export type Params = Record<string, string | number>;
export type TFn = (key: Key, params?: Params) => string;

export function translate(lang: Lang, key: Key, params?: Params): string {
  const dict = DICTS[lang];
  let raw: string = (dict as Record<string, string>)[key] ?? (fr as Record<string, string>)[key] ?? key;
  if (params && typeof params.n === "number") {
    const plural = `${key}_${params.n === 1 || params.n === 0 ? "one" : "other"}`;
    if (plural in dict) raw = (dict as Record<string, string>)[plural];
  }
  return params ? raw.replace(/\{(\w+)\}/g, (_, k) => String(params[k] ?? `{${k}}`)) : raw;
}

const LS = "omnistream.prefs";

export interface Prefs {
  lang: Lang;
  highContrast: boolean;
  notifications: boolean;
}

export function loadPrefs(): Prefs {
  const base: Prefs = {
    lang: typeof navigator !== "undefined" && navigator.language?.toLowerCase().startsWith("en") ? "en" : "fr",
    highContrast: false,
    notifications: true,
  };
  try {
    const saved = JSON.parse(localStorage.getItem(LS) ?? "{}") as Partial<Prefs>;
    return {
      lang: saved.lang === "fr" || saved.lang === "en" ? saved.lang : base.lang,
      highContrast: typeof saved.highContrast === "boolean" ? saved.highContrast : base.highContrast,
      notifications: typeof saved.notifications === "boolean" ? saved.notifications : base.notifications,
    };
  } catch {
    return base;
  }
}

export function savePrefs(p: Prefs) {
  try {
    localStorage.setItem(LS, JSON.stringify(p));
  } catch {
    /* stockage indisponible */
  }
}

export const localeOf = (lang: Lang) => (lang === "fr" ? "fr-FR" : "en-US");
