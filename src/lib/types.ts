// Miroir des types de `omnistream-core::domain`.
export type Platform = "twitch" | "youtube" | "kick" | "tiktok" | "trovo" | { other: string };

export interface Account {
  id: string;
  platform: Platform;
  display_name: string;
  login: string;
}

export interface Channel {
  id: string;
  platform: Platform;
  external_id: string;
  display_name: string;
}

export type EmoteProvider = "twitch" | "seven_tv" | "bttv" | "ffz";

/** Indices en caractères Unicode (fin exclue): utiliser Array.from(text). */
export interface EmoteSpan {
  start: number;
  end: number;
  name: string;
  url: string;
  provider: EmoteProvider;
  zero_width?: boolean;
}

export interface ChatBadge {
  set_id: string;
  version: string;
}

export interface ChatMessage {
  id: string;
  channel_id: string;
  platform: Platform;
  author: {
    external_id: string;
    display_name: string;
    is_moderator: boolean;
    is_subscriber: boolean;
  };
  text: string;
  timestamp_ms: number;
  emotes?: EmoteSpan[];
  badges?: ChatBadge[];
}

export type ModerationKind =
  | { kind: "delete_message"; message_id: string }
  | { kind: "timeout"; user_id: string; seconds: number }
  | { kind: "ban"; user_id: string; reason?: string }
  | { kind: "unban"; user_id: string };

export type ModerationAction = { channel: Channel } & ModerationKind;

export type DestState = "idle" | "waiting_signal" | "connecting" | "live" | "retrying" | "error";

export interface DestinationStatus {
  id: string;
  state: DestState;
  error: string | null;
  attempt: number;
  bitrate_kbps: number | null;
}

/** Destination telle que renvoyée par le backend : la clé de stream n'en sort jamais. */
export interface StreamDestination {
  id: string;
  platform: Platform;
  name: string;
  rtmp_url: string;
  has_key: boolean;
  enabled: boolean;
  status: DestinationStatus;
}

export interface DestinationInput {
  id?: string;
  platform: Platform;
  name: string;
  rtmp_url: string;
  /** Absent : la clé existante est conservée. */
  stream_key?: string;
  enabled: boolean;
}

export interface IngestStatus {
  state: "stopped" | "waiting" | "live" | "error";
  width: number | null;
  height: number | null;
  fps: number | null;
  bitrate_kbps: number | null;
  video_codec: string | null;
  error: string | null;
}

export interface FfmpegStatus {
  found: boolean;
  path: string | null;
  version: string | null;
  message: string | null;
}

export interface StreamingState {
  live: boolean;
  ffmpeg: FfmpegStatus;
  ingest: { url: string; key: string; port: number; status: IngestStatus };
  title: string;
}

export type StreamingEvent =
  | { type: "ingest"; data: IngestStatus }
  | { type: "destination"; data: DestinationStatus }
  | { type: "live"; data: boolean };

export interface TitleResult {
  platform: string;
  ok: boolean;
  message: string;
}

export type StreamEventKind =
  | { type: "follow" }
  | { type: "sub"; months: number }
  | { type: "gift"; recipient: string | null }
  | { type: "raid"; viewers: number }
  | { type: "donation"; amount_cents: number; currency: string }
  | { type: "bits"; amount: number };

export type StreamEventType = StreamEventKind["type"];

export type StreamEvent = {
  id: string;
  channel_id: string;
  platform: Platform;
  who: string;
  message: string | null;
  timestamp_ms: number;
} & StreamEventKind;

// Historique des sessions (miroir de `omnistream-core::db::history`).
export interface SessionSummary {
  id: string;
  started_at_ms: number;
  /** `null` tant que la session est en cours. */
  ended_at_ms: number | null;
  peak_viewers: number;
  avg_viewers: number;
  event_count: number;
  events_by_kind: Partial<Record<StreamEventType, number>>;
}

/** Somme brute des spectateurs à un sondage (les doublons entre plateformes ne sont pas retirés). */
export interface ViewerPoint {
  ts_ms: number;
  total: number;
  by_platform: Record<string, number>;
}

export interface SessionDetail {
  summary: SessionSummary;
  points: ViewerPoint[];
  events: StreamEvent[];
}

export interface StreamInfo {
  channel_id: string;
  title: string;
  viewers: number;
  started_at: string;
}

export interface Category {
  id: string;
  name: string;
}

export interface StreamInfoUpdate {
  title?: string | null;
  category?: string | null;
  tags?: string[] | null;
}

export interface PlatformUpdateResult {
  channel_id: string;
  platform: Platform;
  ok: boolean;
  error: string | null;
}

export interface StreamPreset {
  id: string;
  name: string;
  title: string;
  category: string;
  tags: string[];
}

export interface DeviceCode {
  device_code: string;
  user_code: string;
  verification_uri: string;
  expires_in: number;
  interval: number;
}

// Automod : miroir de `omnistream-core::automod`.
export type RuleKind =
  | { type: "regex"; pattern: string }
  | { type: "banned_words"; words: string[] }
  | { type: "links"; allowed_domains: string[] }
  | { type: "caps"; min_letters: number; ratio_percent: number }
  | { type: "repeat"; min_words: number; char_run: number }
  | { type: "cross_platform_spam"; window_secs: number; min_channels: number };

export type RuleAction = { type: "flag" } | { type: "delete" } | { type: "timeout"; seconds: number };

export interface AutomodRule {
  id: string;
  name: string;
  enabled: boolean;
  score: number;
  kind: RuleKind;
  action: RuleAction;
  /** Vide = toutes les plateformes. */
  platforms: string[];
}

export interface AutomodVerdict {
  matches: { rule_id: string; name: string; score: number; action: RuleAction }[];
  score: number;
}

export interface TrovoSettings {
  client_id: string | null;
  has_secret: boolean;
  redirect_uri: string;
}

// --- Automations ---
export type Role = "everyone" | "subscriber" | "moderator" | "broadcaster";

export type Target = { kind: "source" } | { kind: "all" } | { kind: "platforms"; platforms: Platform[] };

export type Trigger =
  | { type: "message"; contains: string }
  | { type: "follow" }
  | { type: "sub" }
  | { type: "raid"; min_viewers: number }
  | { type: "timer"; every_secs: number }
  | { type: "stream_start" }
  | { type: "stream_end" };

export type AutomationAction =
  | { type: "send_message"; target: Target; text: string }
  | { type: "timeout"; seconds: number }
  | { type: "delete_message" }
  | { type: "ban"; reason: string | null };

export interface Rule {
  id: string;
  name: string;
  enabled: boolean;
  trigger: Trigger;
  action: AutomationAction;
  platforms: Platform[];
  cooldown_secs: number;
}

export interface ChatCommand {
  id: string;
  name: string;
  response: string;
  enabled: boolean;
  cooldown_secs: number;
  user_cooldown_secs: number;
  min_role: Role;
  platforms: Platform[];
  reply: Target;
}

export interface AutomationLogEntry {
  at_ms: number;
  origin: string;
  summary: string;
  error: string | null;
}

export interface Settings {
  twitch_client_id: string | null;
}

// Bans synchronisés (miroir de `omnistream-core::bansync`).
export interface IdentityLink {
  person: string;
  platform: Platform;
  user_id: string;
  display_name: string;
}

export type SyncKind = "ban" | "unban";

export interface SyncRequest {
  source_platform: Platform;
  source_user_id: string;
  source_name: string;
  kind: SyncKind;
  reason: string | null;
  channel_ids: string[] | null;
}

export interface PreviewItem {
  channel: Channel;
  user_ids: string[];
  status: "ready" | "no_identity";
}

export interface QueueItem {
  id: number;
  channel_id: string;
  channel_name: string;
  platform: Platform;
  user_id: string;
  status: "pending" | "done" | "failed";
  attempts: number;
  last_error: string | null;
  updated_at_ms: number;
}

export interface BanJob {
  id: string;
  kind: SyncKind;
  source_name: string;
  reason: string | null;
  created_at_ms: number;
  items: QueueItem[];
}
