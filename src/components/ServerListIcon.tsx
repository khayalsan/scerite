import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { HardDrive, Network } from "lucide-react";

export function ServerListIcon({ profileId, remote }: { profileId: string; remote: boolean }) {
  const [iconUrl, setIconUrl] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    let url: string | null = null;
    void invoke<number[] | null>(remote ? "read_remote_server_icon" : "read_local_server_icon", { profileId })
      .then((bytes) => {
        if (!active || !bytes) return;
        url = URL.createObjectURL(new Blob([Uint8Array.from(bytes)], { type: "image/png" }));
        setIconUrl(url);
      })
      .catch(() => { if (active) setIconUrl(null); });
    return () => { active = false; if (url) URL.revokeObjectURL(url); };
  }, [profileId, remote]);

  return <span className={remote ? "server-icon server-icon-remote" : "server-icon server-icon-local"} aria-hidden="true">
    {iconUrl ? <img src={iconUrl} alt="" onError={() => setIconUrl(null)} /> : remote ? <Network size={18} /> : <HardDrive size={18} />}
  </span>;
}
