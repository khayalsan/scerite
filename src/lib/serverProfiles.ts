import { invoke } from "@tauri-apps/api/core";

export type LocalLaunchConfig = { executable: string; arguments: string[] };

export type ServerTarget =
  | { kind: "local"; directory: string; launch: LocalLaunchConfig }
  | { kind: "remote"; host: string; port: number; username: string; identityFile: string | null; directory: string; launch: LocalLaunchConfig };

export interface ServerProfile {
  id: string;
  name: string;
  target: ServerTarget;
}

export const serverProfiles = {
  list: () => invoke<ServerProfile[]>("list_server_profiles"),
  create: (name: string, target: ServerTarget) =>
    invoke<ServerProfile>("create_server_profile", { name, target }),
  update: (id: string, name: string, target: ServerTarget) =>
    invoke<ServerProfile>("update_server_profile", { id, name, target }),
  delete: (id: string) => invoke<void>("delete_server_profile", { id }),
};
