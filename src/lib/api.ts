import { invoke } from "@tauri-apps/api/core";
import type { Account, StreamDestination } from "./types";

export const api = {
  listAccounts: () => invoke<Account[]>("list_accounts"),
  listDestinations: () => invoke<StreamDestination[]>("list_destinations"),
  saveDestination: (destination: StreamDestination) =>
    invoke<void>("save_destination", { destination }),
  startDestination: (id: string) => invoke<void>("start_destination", { id }),
  stopDestination: (id: string) => invoke<void>("stop_destination", { id }),
};
