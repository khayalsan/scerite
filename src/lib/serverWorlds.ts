import { invoke } from "@tauri-apps/api/core";

export interface WorldRule { name: string; value: string }
export interface WorldSettings { actualSeed: string | null; gamerules: WorldRule[] }
export interface WorldBackup { name: string; path: string; kind: "zip" | "preserved"; modified: number | null; automatic: boolean }
export interface AutoBackupSettings { enabled: boolean; keep: number }

const command = (remote: boolean, action: string) => `${action}_${remote ? "remote" : "local"}_world`;

export const serverWorlds = {
  autoBackupSettings: (profileId: string) =>
    invoke<AutoBackupSettings>("read_auto_backup_settings", { profileId }),
  saveAutoBackupSettings: (profileId: string, enabled: boolean, keep: number) =>
    invoke<AutoBackupSettings>("save_auto_backup_settings", { profileId, enabled, keep }),
  autoBackup: (remote: boolean, profileId: string) =>
    invoke<string | null>(`auto_backup_${remote ? "remote" : "local"}_world`, { profileId }),
  deleteZipBackup: (profileId: string, backup: WorldBackup) =>
    invoke<void>("delete_world_zip_backup", { profileId, worldName: backup.name, backupPath: backup.path }),
  settings: (remote: boolean, profileId: string, worldName: string) =>
    invoke<WorldSettings>(`read_${remote ? "remote" : "local"}_world_settings`, { profileId, worldName }),
  updateRule: (remote: boolean, profileId: string, worldName: string, name: string, value: string) =>
    invoke<void>(`update_${remote ? "remote" : "local"}_world_gamerule`, { profileId, worldName, name, value }),
  backups: (remote: boolean, profileId: string) =>
    invoke<WorldBackup[]>(`list_${remote ? "remote" : "local"}_world_backups`, { profileId }),
  restore: (remote: boolean, profileId: string, backup: WorldBackup) =>
    invoke<string>(`restore_${remote ? "remote" : "local"}_world_backup`, {
      profileId, worldName: backup.name, backupKind: backup.kind, backupPath: backup.path,
    }),
  backup: (remote: boolean, profileId: string, worldName: string) =>
    invoke<string>(command(remote, "backup"), { profileId, worldName }),
  download: (remote: boolean, profileId: string, worldName: string, destination: string) =>
    invoke<void>(command(remote, "download"), { profileId, worldName, destination }),
  upload: (remote: boolean, profileId: string, worldName: string, source: string) =>
    invoke<string>(command(remote, "upload"), { profileId, worldName, source }),
  import: (remote: boolean, profileId: string, worldName: string, source: string) =>
    invoke<string>(command(remote, "import"), { profileId, worldName, source }),
  regenerate: (remote: boolean, profileId: string, worldName: string, seed: string, levelType: string, generateStructures: boolean) =>
    invoke<string>(command(remote, "regenerate"), { profileId, worldName, seed, levelType, generateStructures }),
};
