import { invoke } from "@tauri-apps/api/core";
import type { ChatMessage } from "./types";

export interface OverlayInfo {
  running: boolean;
  port: number;
  token: string;
  chat_url: string;
  error: string | null;
}

export type OverlayTheme = "orbite" | "minimal" | "clair";

export interface OverlaySettings {
  theme: OverlayTheme;
  custom_css: string;
  max_messages: number;
  show_platform: boolean;
  font_size: number;
}

export const overlayApi = {
  info: () => invoke<OverlayInfo>("overlay_info"),
  setPort: (port: number) => invoke<OverlayInfo>("overlay_set_port", { port }),
  regenerateToken: () => invoke<OverlayInfo>("overlay_regenerate_token"),
  getSettings: () => invoke<OverlaySettings>("overlay_get_settings"),
  setSettings: (settings: OverlaySettings) => invoke<OverlaySettings>("overlay_set_settings", { settings }),
  pin: (message: ChatMessage) => invoke<void>("overlay_pin", { message }),
  unpin: () => invoke<void>("overlay_unpin"),
  highlight: (messageId: string, on: boolean) => invoke<void>("overlay_highlight", { messageId, on }),
  clear: () => invoke<void>("overlay_clear"),
  testMessage: () => invoke<void>("overlay_test_message"),
};
