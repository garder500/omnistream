import { invoke } from "@tauri-apps/api/core";
import type {
  Account,
  Channel,
  DeviceCode,
  ModerationKind,
  Settings,
  StreamDestination,
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

  listDestinations: () => invoke<StreamDestination[]>("list_destinations"),
  saveDestination: (destination: StreamDestination) => invoke<void>("save_destination", { destination }),
  startDestination: (id: string) => invoke<void>("start_destination", { id }),
  stopDestination: (id: string) => invoke<void>("stop_destination", { id }),
};
