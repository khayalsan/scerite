import { invoke } from "@tauri-apps/api/core";

export interface ServerLogSnapshot {
  text: string;
  truncated: boolean;
}

export const serverLogs = {
  read: (remote: boolean, profileId: string, name: string) =>
    invoke<ServerLogSnapshot>(remote ? "read_remote_server_log" : "read_local_server_log", { profileId, name }),
};
