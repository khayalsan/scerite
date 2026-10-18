import { invoke } from "@tauri-apps/api/core";

export type LocalLaunchConfig = { executable: string; arguments: string[]; managedJava?: string | null };

export type ServerTarget =
  | { kind: "local"; directory: string; launch: LocalLaunchConfig }
  | { kind: "remote"; environmentId: string; directory: string; launch: LocalLaunchConfig };

export interface ServerEnvironment {
  id: string;
  name: string;
  rootDirectory: string;
  host: string;
  port: number;
  username: string;
  identityFile: string | null;
}

export interface ServerProfile {
  id: string;
  name: string;
  target: ServerTarget;
  installedSoftware?: string | null;
}

export const serverProfiles = {
  list: () => invoke<ServerProfile[]>("list_server_profiles"),
  create: (name: string, target: ServerTarget, autoDirectory = false) =>
    invoke<ServerProfile>("create_server_profile", { name, target, autoDirectory }),
  update: (id: string, name: string, target: ServerTarget) =>
    invoke<ServerProfile>("update_server_profile", { id, name, target }),
  delete: (id: string) => invoke<void>("delete_server_profile", { id }),
};

export const serverEnvironments = {
  list: () => invoke<ServerEnvironment[]>("list_server_environments"),
  create: (name: string, rootDirectory: string, host: string, port: number, username: string, identityFile: string | null) =>
    invoke<ServerEnvironment>("create_server_environment", { name, rootDirectory, host, port, username, identityFile }),
  update: (id: string, name: string, rootDirectory: string, host: string, port: number, username: string, identityFile: string | null) =>
    invoke<ServerEnvironment>("update_server_environment", { id, name, rootDirectory, host, port, username, identityFile }),
  delete: (id: string) => invoke<void>("delete_server_environment", { id }),
};

export const localInstallRoot = {
  get: () => invoke<string | null>("get_local_install_root"),
  set: (rootDirectory: string) => invoke<string>("set_local_install_root", { rootDirectory }),
};

export const appSettings = {
  getAutoAcceptEula: () => invoke<boolean>("get_auto_accept_eula"),
  setAutoAcceptEula: (enabled: boolean) => invoke<boolean>("set_auto_accept_eula", { enabled }),
};
