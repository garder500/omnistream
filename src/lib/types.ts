// Miroir des types de `omnistream-core::domain`.
export type Platform = "twitch" | "youtube" | "kick" | "tiktok" | { other: string };

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

export interface DeviceCode {
  device_code: string;
  user_code: string;
  verification_uri: string;
  expires_in: number;
  interval: number;
}

export interface Settings {
  twitch_client_id: string | null;
}
