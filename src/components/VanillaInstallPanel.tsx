import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "./ui/select";

type Progress = { profileId: string; phase: string; downloaded: number; total: number };

export function VanillaInstallPanel({ profileId, remote, rootConfigured, canInstall }: { profileId: string; remote: boolean; rootConfigured: boolean; canInstall: boolean }) {
  const [versions, setVersions] = useState<string[]>([]);
  const [version, setVersion] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<string | null>(null);
  const [progress, setProgress] = useState<Progress | null>(null);

  useEffect(() => {
    let active = true;
    invoke<string[]>("list_vanilla_versions").then((items) => {
      if (active) { setVersions(items); setVersion(items[0] ?? ""); setLoading(false); }
    }).catch((cause) => {
      if (active) { setError(String(cause)); setLoading(false); }
    });
    return () => { active = false; };
  }, []);

  useEffect(() => {
    const listener = listen<Progress>("vanilla-install-progress", (event) => {
      if (event.payload.profileId === profileId) setProgress(event.payload);
    });
    return () => { void listener.then((unlisten) => unlisten()); };
  }, [profileId]);

  async function install() {
    setBusy(true);
    setError(null);
    setResult(null);
    setProgress(null);
    try {
      setResult(await invoke<string>("install_vanilla_version", { profileId, versionId: version }));
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }

  const phase = progress?.phase === "downloading" && progress.total
    ? `Downloading ${Math.round(progress.downloaded / progress.total * 100)}%`
    : progress?.phase === "uploading" ? "Uploading to remote cache…"
    : progress?.phase === "installing" ? "Copying into server folder…"
    : progress?.phase === "cached" ? "Using cached jar…"
    : progress?.phase === "checking" ? "Checking official version…"
    : "Preparing installation…";

  return <div className="install-panel">
    <div>
      <h2>Install vanilla Minecraft</h2>
      <p>Download an official release into cache/minecraft, then install it as server.jar in this server’s folder. Existing files are preserved.</p>
    </div>
    <div className="install-form">
      <label htmlFor="vanilla-version">Minecraft version</label>
      <Select value={version} onValueChange={(value) => setVersion(value ?? "")} disabled={loading || busy}>
        <SelectTrigger id="vanilla-version" className="install-version-trigger"><SelectValue>{(selected) => selected || "Choose a version"}</SelectValue></SelectTrigger>
        <SelectContent>{versions.map((item) => <SelectItem key={item} value={item}>{item}</SelectItem>)}</SelectContent>
      </Select>
      <button className="button button-primary" type="button" disabled={loading || busy || !version || !rootConfigured || !canInstall} onClick={() => void install()}>{busy ? "Installing…" : "Install version"}</button>
    </div>
    {loading && <p role="status">Loading official releases…</p>}
    {!rootConfigured && <p className="form-error">Set the {remote ? "remote environment" : "local"} Minecraft root on the Servers page first.</p>}
    {rootConfigured && !canInstall && <p className="form-error">Stop the server and confirm its status before installing.</p>}
    {busy && <p role="status">{phase}</p>}
    {error && <p className="form-error" role="alert">{error}</p>}
    {result && <p className="install-success" role="status">{result} Check the profile’s launch arguments before starting.</p>}
  </div>;
}
