// Assistant IA (BYOK) : réglages, suggestions de modération et résumés.
// Désactivé par défaut. Aucune action n'est jamais appliquée sans clic du modérateur.
import { invoke } from "@tauri-apps/api/core";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useStore, type StoredMessage } from "./store";
import type { StreamEvent } from "./types";

export interface AiSettings {
  enabled: boolean;
  has_key: boolean;
  moderation_model: string;
  summary_model: string;
}

export interface AiSettingsInput {
  enabled: boolean;
  /** Consentement explicite à l'envoi du chat à Anthropic (requis pour activer). */
  consent?: boolean;
  /** undefined = conserver la clé, "" = la supprimer. */
  api_key?: string;
  moderation_model: string;
  summary_model: string;
}

export interface AiVerdict {
  spam: number;
  hate: number;
  scam: number;
  reason: string;
  suggestion: "timeout" | "ban" | null;
  timeout_seconds: number;
}

export interface AiSuggestion {
  messageId: string;
  verdict: AiVerdict;
}

export interface AiSummary {
  kind: "live" | "recap";
  text: string;
  at: number;
}

export const aiApi = {
  getSettings: () => invoke<AiSettings>("ai_get_settings"),
  saveSettings: (settings: AiSettingsInput) => invoke<AiSettings>("ai_save_settings", { settings }),
  scoreMessage: (text: string) => invoke<AiVerdict | null>("ai_score_message", { text }),
  summarize: (kind: "live" | "recap", lines: { author: string; text: string }[], events: string[]) =>
    invoke<string>("ai_summarize", { request: { kind, lines, events } }),
};

const MAX_PENDING = 20;
const MAX_SUGGESTIONS = 30;
const SUMMARY_LINES = 300;

interface AiCtx {
  settings: AiSettings | null;
  suggestions: AiSuggestion[];
  /** Messages en attente d'analyse. */
  pending: number;
  summary: AiSummary | null;
  summarizing: "live" | "recap" | null;
  summaryError: string | null;
  save: (input: AiSettingsInput) => Promise<boolean>;
  dropSuggestion: (messageId: string) => void;
  summarize: (kind: "live" | "recap") => Promise<void>;
}

const Ctx = createContext<AiCtx | null>(null);

export function useAi(): AiCtx {
  const c = useContext(Ctx);
  if (!c) throw new Error("useAi hors de l'AiProvider");
  return c;
}

const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e));

export function eventLine(e: StreamEvent): string {
  const what = (() => {
    switch (e.type) {
      case "follow":
        return "follow";
      case "sub":
        return `abonnement (${e.months} mois)`;
      case "gift":
        return "don d'abonnement";
      case "raid":
        return `raid (${e.viewers} spectateurs)`;
      case "donation":
        return `don (${(e.amount_cents / 100).toFixed(2)} ${e.currency})`;
      case "bits":
        return `${e.amount} bits`;
    }
  })();
  return `${e.who} : ${what}`;
}

export function AiProvider({ children }: { children: ReactNode }) {
  const { messages, events, notify } = useStore();
  const [settings, setSettings] = useState<AiSettings | null>(null);
  const [suggestions, setSuggestions] = useState<AiSuggestion[]>([]);
  const [pending, setPending] = useState(0);
  const [summary, setSummary] = useState<AiSummary | null>(null);
  const [summarizing, setSummarizing] = useState<"live" | "recap" | null>(null);
  const [summaryError, setSummaryError] = useState<string | null>(null);

  const seen = useRef(new Set<string>());
  const queue = useRef<StoredMessage[]>([]);
  const busy = useRef(false);
  const generation = useRef(0);
  const modelsRef = useRef({ moderation_model: "claude-haiku-5-5", summary_model: "claude-sonnet-5-5" });
  const enabled = !!settings?.enabled && settings.has_key;

  useEffect(() => {
    aiApi.getSettings().then(setSettings).catch(() => setSettings(null));
  }, []);

  useEffect(() => {
    if (settings) modelsRef.current = { moderation_model: settings.moderation_model, summary_model: settings.summary_model };
  }, [settings]);

  const pump = useCallback(async () => {
    if (busy.current) return;
    busy.current = true;
    const gen = generation.current;
    let failures = 0;
    try {
      while (queue.current.length > 0 && gen === generation.current && failures < 3) {
        const m = queue.current.shift()!;
        setPending(queue.current.length);
        try {
          const verdict = await aiApi.scoreMessage(m.text);
          failures = 0;
          if (verdict?.suggestion && gen === generation.current) {
            setSuggestions((s) => [{ messageId: m.id, verdict }, ...s].slice(0, MAX_SUGGESTIONS));
          }
        } catch (e) {
          if (++failures === 1) notify(`Assistant IA : ${errText(e)}`, true);
        }
      }
      if (failures >= 3) {
        // Disjoncteur : clé refusée, quota, réseau… on désactive plutôt que de marteler l'API.
        queue.current = [];
        setPending(0);
        notify("Assistant IA désactivé après plusieurs erreurs consécutives. Vérifiez votre clé puis réactivez-le.", true);
        setSettings((cur) => (cur ? { ...cur, enabled: false } : cur));
        aiApi
          .saveSettings({ enabled: false, moderation_model: modelsRef.current.moderation_model, summary_model: modelsRef.current.summary_model })
          .then(setSettings)
          .catch(() => {});
      }
    } finally {
      busy.current = false;
    }
    // Des messages ont pu arriver pendant l'arrêt d'une boucle périmée.
    if (queue.current.length > 0 && failures < 3) void pump();
  }, [notify]);

  // Analyse uniquement les messages arrivés *après* l'activation.
  useEffect(() => {
    if (!enabled) {
      generation.current++;
      queue.current = [];
      seen.current = new Set(messages.map((m) => m.id));
      setPending(0);
      setSuggestions((s) => (s.length ? [] : s));
      return;
    }
    const fresh = messages.filter((m) => !seen.current.has(m.id));
    if (fresh.length === 0) return;
    // On ne garde que les identifiants encore présents dans le tampon du store (pas de fuite mémoire).
    const live = new Set(messages.map((m) => m.id));
    seen.current = new Set([...seen.current].filter((id) => live.has(id)));
    fresh.forEach((m) => seen.current.add(m.id));
    const eligible = fresh.filter((m) => !m.author.is_moderator && !m.deleted && !m.resolved);
    queue.current = [...queue.current, ...eligible].slice(-MAX_PENDING);
    setPending(queue.current.length);
    void pump();
  }, [messages, enabled, pump]);

  // Une suggestion devient caduque quand le message est déjà traité ou supprimé.
  useEffect(() => {
    setSuggestions((s) => {
      const open = new Set(messages.filter((m) => !m.resolved && !m.deleted).map((m) => m.id));
      const next = s.filter((x) => open.has(x.messageId));
      return next.length === s.length ? s : next;
    });
  }, [messages]);

  const save = useCallback(
    async (input: AiSettingsInput) => {
      try {
        setSettings(await aiApi.saveSettings(input));
        return true;
      } catch (e) {
        notify(errText(e), true);
        return false;
      }
    },
    [notify],
  );

  const dropSuggestion = useCallback((id: string) => setSuggestions((s) => s.filter((x) => x.messageId !== id)), []);

  const summarize = useCallback(
    async (kind: "live" | "recap") => {
      setSummarizing(kind);
      setSummaryError(null);
      try {
        const lines = messages
          .filter((m) => !m.deleted)
          .slice(-SUMMARY_LINES)
          .map((m) => ({ author: m.author.display_name, text: m.text }));
        const text = await aiApi.summarize(kind, lines, kind === "recap" ? events.map(eventLine) : []);
        setSummary({ kind, text, at: Date.now() });
      } catch (e) {
        setSummaryError(errText(e));
      } finally {
        setSummarizing(null);
      }
    },
    [messages, events],
  );

  const value = useMemo<AiCtx>(
    () => ({ settings, suggestions, pending, summary, summarizing, summaryError, save, dropSuggestion, summarize }),
    [settings, suggestions, pending, summary, summarizing, summaryError, save, dropSuggestion, summarize],
  );
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
