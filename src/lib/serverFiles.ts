import { invoke } from "@tauri-apps/api/core";

export interface ServerFileEntry {
  name: string;
  path: string;
  directory: boolean;
  size: number;
}

export const serverFiles = {
  list: (remote: boolean, profileId: string, relativePath: string) =>
    invoke<ServerFileEntry[]>(remote ? "list_remote_server_files" : "list_local_server_files", { profileId, relativePath }),
  read: (remote: boolean, profileId: string, relativePath: string) =>
    invoke<string>(`read_${remote ? "remote" : "local"}_server_file`, { profileId, relativePath }),
  write: (remote: boolean, profileId: string, relativePath: string, content: string, overwrite: boolean) =>
    invoke<void>(`write_${remote ? "remote" : "local"}_server_file`, { profileId, relativePath, content, overwrite }),
};
