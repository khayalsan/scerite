import { invoke } from "@tauri-apps/api/core";

export type LocalServerState = "stopped" | "running" | "stopping";

export interface LocalServerStatus {
  state: LocalServerState;
  processId: number | null;
}

export interface LocalConsoleSnapshot {
  lines: Array<{ sequence: number; text: string }>;
  nextSequence: number;
}

export const serverSessions = {
  status: (profileId: string) =>
    invoke<LocalServerStatus>("local_server_status", { profileId }),
  start: (profileId: string) =>
    invoke<LocalServerStatus>("start_local_server", { profileId }),
  stop: (profileId: string) =>
    invoke<LocalServerStatus>("stop_local_server", { profileId }),
  console: (profileId: string, afterSequence: number) =>
    invoke<LocalConsoleSnapshot>("local_server_console", { profileId, afterSequence }),
  sendInput: (profileId: string, input: string) =>
    invoke<void>("send_local_console_input", { profileId, input }),
};
