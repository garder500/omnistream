import { invoke } from "@tauri-apps/api/core";
import type {
  Account,
  AutomodRule,
  AutomodVerdict,
  Channel,
  DeviceCode,
  ModerationKind,
  SessionDetail,
  SessionSummary,
  Settings,
  DestinationInput,
  FfmpegStatus,
  TrovoSettings,
  Category,
  PlatformUpdateResult,
  StreamDestination,
  StreamingState,
  TitleResult,
  StreamInfo,
  StreamInfoUpdate,
  StreamPreset,
} from "./types";

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  setTwitchClientId: (clientId: string) => invoke<void>("set_twitch_client_id", { clientId }),

  listAccounts: () => invoke<Account[]>("list_accounts"),
  twitchLoginStart: () => invoke<DeviceCode>("twitch_login_start"),
  getTrovoSettings: () => invoke<TrovoSettings>("get_trovo_settings"),
  setTrovoApp: (clientId: string, clientSecret: string) => invoke<void>("set_trovo_app", { clientId, clientSecret }),
  trovoLoginStart: () => invoke<string>("trovo_login_start"),
  removeAccount: (id: string) => invoke<void>("remove_account", { id }),

  listChannels: () => invoke<Channel[]>("list_channels"),
  addChannel: (platform: string, name: string) => invoke<Channel>("add_channel", { platform, name }),
  removeChannel: (channelId: string) => invoke<void>("remove_channel", { channelId }),
  sendMessage: (channelIds: string[], text: string) => invoke<void>("send_message", { channelIds, text }),
  moderate: (channelId: string, kind: ModerationKind) => invoke<void>("moderate", { channelId, kind }),
  listAutomodRules: () => invoke<AutomodRule[]>("list_automod_rules"),
  saveAutomodRule: (rule: AutomodRule) => invoke<void>("save_automod_rule", { rule }),
  deleteAutomodRule: (id: string) => invoke<void>("delete_automod_rule", { id }),
  testAutomod: (text: string, platform: string) => invoke<AutomodVerdict>("test_automod", { text, platform }),
  streamInfos: () => invoke<StreamInfo[]>("stream_infos"),

  streamingState: () => invoke<StreamingState>("streaming_state"),

  listSessions: () => invoke<SessionSummary[]>("list_sessions"),
  sessionDetail: (id: string) => invoke<SessionDetail>("session_detail", { id }),
  /** Écrit le CSV dans le dossier Téléchargements ; renvoie le chemin du fichier. */
  saveSessionCsv: (id: string) => invoke<string>("save_session_csv", { id }),
  deleteSession: (id: string) => invoke<void>("delete_session", { id }),

  listDestinations: () => invoke<StreamDestination[]>("list_destinations"),
  saveDestination: (destination: DestinationInput) => invoke<StreamDestination>("save_destination", { destination }),
  deleteDestination: (id: string) => invoke<void>("delete_destination", { id }),
  goLive: () => invoke<void>("go_live"),
  stopLive: () => invoke<void>("stop_live"),
  setIngestPort: (port: number) => invoke<void>("set_ingest_port", { port }),
  setFfmpegPath: (path: string) => invoke<FfmpegStatus>("set_ffmpeg_path", { path }),
  setStreamTitle: (title: string) => invoke<TitleResult[]>("set_stream_title", { title }),
  twitchCurrentTitle: () => invoke<string | null>("twitch_current_title"),

  updateStreamInfo: (channelIds: string[], update: StreamInfoUpdate) =>
    invoke<PlatformUpdateResult[]>("update_stream_info", { channelIds, update }),
  searchCategories: (query: string) => invoke<Category[]>("search_categories", { query }),
  listPresets: () => invoke<StreamPreset[]>("list_presets"),
  savePreset: (preset: StreamPreset) => invoke<void>("save_preset", { preset }),
  deletePreset: (id: string) => invoke<void>("delete_preset", { id }),
};
