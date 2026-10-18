import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { ask, message } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import "./StartupUpdater.css";

let startupCheck: ReturnType<typeof check> | undefined;

export function StartupUpdater() {
  const [progress, setProgress] = useState<{ label: string; percent?: number } | null>(null);

  useEffect(() => {
    if (!isTauri() || import.meta.env.DEV) return;

    let active = true;
    void (async () => {
      let update;
      try {
        startupCheck ??= check({ timeout: 15_000 });
        update = await startupCheck;
      } catch (error) {
        // A missing release or an offline connection should not interrupt startup.
        console.warn("Could not check for updates:", error);
        return;
      }
      if (!update || !active) return;

      try {
        const confirmed = await ask(`Scerite ${update.version} is available. Update now?`, {
          title: "Update available",
          kind: "info",
          okLabel: "Update",
          cancelLabel: "Later",
        });
        if (!confirmed || !active) return;

        let downloaded = 0;
        let total: number | undefined;
        setProgress({ label: "Downloading update…" });
        await update.downloadAndInstall((event) => {
          if (!active) return;
          if (event.event === "Started") total = event.data.contentLength;
          if (event.event === "Progress") downloaded += event.data.chunkLength;
          if (event.event === "Finished") {
            setProgress({ label: "Installing update…" });
          } else {
            const percent = total ? Math.min(100, Math.floor((downloaded / total) * 100)) : undefined;
            setProgress((current) => current?.label === "Downloading update…" && current.percent === percent
              ? current
              : { label: "Downloading update…", percent });
          }
        });
        if (active) await relaunch();
      } catch (error) {
        if (!active) return;
        setProgress(null);
        await message(`The update could not be completed. ${String(error)}`, {
          title: "Update failed",
          kind: "error",
        });
      }
    })();

    return () => { active = false; };
  }, []);

  if (!progress) return null;
  return (
    <div className="update-progress" role="status" aria-live="polite">
      <strong>{progress.label}</strong>
      {progress.percent !== undefined && <span>{progress.percent}%</span>}
      {progress.percent !== undefined && <progress value={progress.percent} max={100} aria-label="Update download progress" />}
    </div>
  );
}
