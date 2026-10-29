import { listen } from "@tauri-apps/api/event";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api } from "./api";
import type { Account, Channel, ChatMessage, ModerationKind, StreamEvent, StreamInfo } from "./types";

const MAX_MESSAGES = 500;
const MAX_EVENTS = 50;

export interface StoredMessage extends ChatMessage {
  flag: string | null;
  deleted?: boolean;
  /** Message traité (ou ignoré) depuis la file de modération. */
  resolved?: boolean;
}

export interface Toast {
  id: number;
  text: string;
  error: boolean;
}

interface Store {
  accounts: Account[];
  channels: Channel[];
  connected: Record<string, boolean>;
  messages: StoredMessage[];
  events: StreamEvent[];
  infos: StreamInfo[];
  msgPerMin: number;
  toasts: Toast[];
  modLog: string[];
  refreshAccounts: () => Promise<void>;
  refreshChannels: () => Promise<void>;
  addChannel: (platform: string, name: string) => Promise<void>;
  removeChannel: (id: string) => Promise<void>;
  send: (channelIds: string[], text: string) => Promise<boolean>;
  moderate: (message: StoredMessage, kind: ModerationKind, label: string) => Promise<boolean>;
  dismiss: (messageId: string) => void;
  notify: (text: string, error?: boolean) => void;
}

const Ctx = createContext<Store | null>(null);

export function useStore(): Store {
  const s = useContext(Ctx);
  if (!s) throw new Error("useStore hors du StoreProvider");
  return s;
}

const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e));

export function StoreProvider({ children }: { children: ReactNode }) {
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [channels, setChannels] = useState<Channel[]>([]);
  const [connected, setConnected] = useState<Record<string, boolean>>({});
  const [messages, setMessages] = useState<StoredMessage[]>([]);
  const [events, setEvents] = useState<StreamEvent[]>([]);
  const [infos, setInfos] = useState<StreamInfo[]>([]);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [modLog, setModLog] = useState<string[]>([]);
  const [now, setNow] = useState(() => Date.now());
  const arrivals = useRef<number[]>([]);
  const toastId = useRef(0);

  const notify = useCallback((text: string, error = false) => {
    const id = ++toastId.current;
    setToasts((t) => [...t, { id, text, error }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), 6000);
  }, []);

  const refreshAccounts = useCallback(async () => setAccounts(await api.listAccounts()), []);
  const refreshChannels = useCallback(async () => setChannels(await api.listChannels()), []);

  useEffect(() => {
    refreshAccounts().catch((e) => notify(errText(e), true));
    refreshChannels().catch((e) => notify(errText(e), true));
  }, [refreshAccounts, refreshChannels, notify]);

  // Événements poussés par le backend. `cancelled` évite les doublons avec React.StrictMode.
  useEffect(() => {
    let cancelled = false;
    const unlisteners: Promise<() => void>[] = [];
    const on = <T,>(name: string, handler: (payload: T) => void) => {
      const p = listen<T>(name, (e) => !cancelled && handler(e.payload));
      unlisteners.push(p);
    };

    on<{ message: ChatMessage; flag: string | null }>("chat://message", ({ message, flag }) => {
      arrivals.current.push(Date.now());
      setMessages((ms) => [...ms.slice(-(MAX_MESSAGES - 1)), { ...message, flag }]);
    });
    on<StreamEvent>("chat://event", (e) => setEvents((es) => [e, ...es].slice(0, MAX_EVENTS)));
    on<{ message_id: string }>("chat://deleted", ({ message_id }) =>
      setMessages((ms) => ms.map((m) => (m.id === message_id ? { ...m, deleted: true } : m))),
    );
    on<{ channel_id: string; connected: boolean }>("chat://status", ({ channel_id, connected }) =>
      setConnected((c) => ({ ...c, [channel_id]: connected })),
    );
    on<unknown>("account://changed", () => {
      refreshAccounts();
    });
    on<string>("account://error", (msg) => notify(msg, true));

    return () => {
      cancelled = true;
      unlisteners.forEach((u) => u.then((fn) => fn()));
    };
  }, [refreshAccounts, notify]);

  // Cadence des messages (fenêtre glissante d'une minute) et infos de live.
  useEffect(() => {
    const t = setInterval(() => {
      const cutoff = Date.now() - 60_000;
      arrivals.current = arrivals.current.filter((a) => a >= cutoff);
      setNow(Date.now());
    }, 5000);
    return () => clearInterval(t);
  }, []);

  useEffect(() => {
    if (channels.length === 0 || accounts.length === 0) {
      setInfos([]);
      return;
    }
    let cancelled = false;
    const load = () =>
      api
        .streamInfos()
        .then((i) => !cancelled && setInfos(i))
        .catch(() => {});
    load();
    const t = setInterval(load, 30_000);
    return () => {
      cancelled = true;
      clearInterval(t);
    };
  }, [channels, accounts]);

  const addChannel = useCallback(
    async (platform: string, name: string) => {
      try {
        await api.addChannel(platform, name);
        await refreshChannels();
      } catch (e) {
        notify(errText(e), true);
      }
    },
    [refreshChannels, notify],
  );

  const removeChannel = useCallback(
    async (id: string) => {
      try {
        await api.removeChannel(id);
        setMessages((ms) => ms.filter((m) => m.channel_id !== id));
        await refreshChannels();
      } catch (e) {
        notify(errText(e), true);
      }
    },
    [refreshChannels, notify],
  );

  const send = useCallback(
    async (channelIds: string[], text: string) => {
      try {
        await api.sendMessage(channelIds, text);
        return true;
      } catch (e) {
        notify(errText(e), true);
        return false;
      }
    },
    [notify],
  );

  const moderate = useCallback(
    async (message: StoredMessage, kind: ModerationKind, label: string) => {
      try {
        await api.moderate(message.channel_id, kind);
        setModLog((l) => [`${label} · ${message.author.display_name}`, ...l].slice(0, 20));
        setMessages((ms) => ms.map((m) => (m.id === message.id ? { ...m, resolved: true } : m)));
        return true;
      } catch (e) {
        notify(errText(e), true);
        return false;
      }
    },
    [notify],
  );

  const dismiss = useCallback(
    (messageId: string) => setMessages((ms) => ms.map((m) => (m.id === messageId ? { ...m, resolved: true } : m))),
    [],
  );

  const value = useMemo<Store>(
    () => ({
      accounts,
      channels,
      connected,
      messages,
      events,
      infos,
      msgPerMin: arrivals.current.filter((a) => a >= now - 60_000).length,
      toasts,
      modLog,
      refreshAccounts,
      refreshChannels,
      addChannel,
      removeChannel,
      send,
      moderate,
      dismiss,
      notify,
    }),
    [accounts, channels, connected, messages, events, infos, now, toasts, modLog, refreshAccounts, refreshChannels, addChannel, removeChannel, send, moderate, dismiss, notify],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function flaggedQueue(messages: StoredMessage[]): StoredMessage[] {
  return messages.filter((m) => m.flag && !m.resolved && !m.deleted).reverse();
}

export function formatAge(ms: number): string {
  const s = Math.max(0, Math.round((Date.now() - ms) / 1000));
  if (s < 60) return `${s} s`;
  if (s < 3600) return `${Math.round(s / 60)} min`;
  return `${Math.round(s / 3600)} h`;
}
