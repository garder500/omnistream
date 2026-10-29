import { invoke } from "@tauri-apps/api/core";
import type {
  Account,
  Channel,
  DeviceCode,
  ModerationKind,
  Settings,
  DestinationInput,
  FfmpegStatus,
  StreamDestination,
  StreamingState,
  TitleResult,
  StreamInfo,
} from "./types";

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  setTwitchClientId: (clientId: string) => invoke<void>("set_twitch_client_id", { clientId }),

  listAccounts: () => invoke<Account[]>("list_accounts"),
  twitchLoginStart: () => invoke<DeviceCode>("twitch_login_start"),
  removeAccount: (id: string) => invoke<void>("remove_account", { id }),

  listChannels: () => invoke<Channel[]>("list_channels"),
  addChannel: (platform: string, name: string) => invoke<Channel>("add_channel", { platform, name }),
  removeChannel: (channelId: string) => invoke<void>("remove_channel", { channelId }),
  sendMessage: (channelIds: string[], text: string) => invoke<void>("send_message", { channelIds, text }),
  moderate: (channelId: string, kind: ModerationKind) => invoke<void>("moderate", { channelId, kind }),
  streamInfos: () => invoke<StreamInfo[]>("stream_infos"),

  streamingState: () => invoke<StreamingState>("streaming_state"),
  listDestinations: () => invoke<StreamDestination[]>("list_destinations"),
  saveDestination: (destination: DestinationInput) => invoke<StreamDestination>("save_destination", { destination }),
  deleteDestination: (id: string) => invoke<void>("delete_destination", { id }),
  goLive: () => invoke<void>("go_live"),
  stopLive: () => invoke<void>("stop_live"),
  setIngestPort: (port: number) => invoke<void>("set_ingest_port", { port }),
  setFfmpegPath: (path: string) => invoke<FfmpegStatus>("set_ffmpeg_path", { path }),
  setStreamTitle: (title: string) => invoke<TitleResult[]>("set_stream_title", { title }),
  twitchCurrentTitle: () => invoke<string | null>("twitch_current_title"),
};
