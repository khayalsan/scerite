import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { HardDrive, Network } from "lucide-react";
import { displayCache } from "../lib/displayCache";

export function ServerListIcon({ profileId, remote }: { profileId: string; remote: boolean }) {
  const [iconUrl, setIconUrl] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    let url: string | null = null;
    const show = (bytes: number[] | null) => {
      if (!active) return;
      if (url) URL.revokeObjectURL(url);
      url = bytes ? URL.createObjectURL(new Blob([Uint8Array.from(bytes)], { type: "image/png" })) : null;
      setIconUrl(url);
    };
    void (async () => {
      if (remote) {
        try { const cached = await displayCache.readIcon(profileId); if (cached) show(cached); }
        catch { /* A missing or damaged cache must not delay the server read. */ }
      }
      return invoke<number[] | null>(remote ? "read_remote_server_icon" : "read_local_server_icon", { profileId });
    })()
      .then((bytes) => {
        if (!active) return;
        show(bytes);
        if (remote) void displayCache.writeIcon(profileId, bytes).catch(() => {});
      })
      .catch(() => { /* Keep a cached icon visible if the server is unreachable. */ });
    return () => { active = false; if (url) URL.revokeObjectURL(url); };
  }, [profileId, remote]);

  return <span className={remote ? "server-icon server-icon-remote" : "server-icon server-icon-local"} aria-hidden="true">
    {iconUrl ? <img src={iconUrl} alt="" onError={() => setIconUrl(null)} /> : remote ? <Network size={18} /> : <HardDrive size={18} />}
  </span>;
}
