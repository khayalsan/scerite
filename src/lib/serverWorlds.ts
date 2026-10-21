import { invoke } from "@tauri-apps/api/core";

export interface WorldRule { name: string; value: string }
export interface WorldSettings { actualSeed: string | null; gamerules: WorldRule[] }
export interface WorldBackup { name: string; path: string; kind: "zip" | "preserved"; modified: number | null }

const command = (remote: boolean, action: string) => `${action}_${remote ? "remote" : "local"}_world`;

export const serverWorlds = {
  settings: (remote: boolean, profileId: string, worldName: string) =>
    invoke<WorldSettings>(`read_${remote ? "remote" : "local"}_world_settings`, { profileId, worldName }),
  backups: (remote: boolean, profileId: string) =>
    invoke<WorldBackup[]>(`list_${remote ? "remote" : "local"}_world_backups`, { profileId }),
  backup: (remote: boolean, profileId: string, worldName: string) =>
    invoke<string>(command(remote, "backup"), { profileId, worldName }),
  download: (remote: boolean, profileId: string, worldName: string, destination: string) =>
    invoke<void>(command(remote, "download"), { profileId, worldName, destination }),
  upload: (remote: boolean, profileId: string, worldName: string, source: string) =>
    invoke<string>(command(remote, "upload"), { profileId, worldName, source }),
  regenerate: (remote: boolean, profileId: string, worldName: string, seed: string, levelType: string, generateStructures: boolean) =>
    invoke<string>(command(remote, "regenerate"), { profileId, worldName, seed, levelType, generateStructures }),
};
