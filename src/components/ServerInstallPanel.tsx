import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "./ui/select";

type Progress = { profileId: string; phase: string; downloaded: number; total: number };

const software = [
  { id: "vanilla", label: "Vanilla" },
  { id: "paper", label: "Paper" },
  { id: "purpur", label: "Purpur" },
  { id: "fabric", label: "Fabric" },
  { id: "quilt", label: "Quilt" },
  { id: "forge", label: "Forge" },
  { id: "neoforge", label: "NeoForge" },
  { id: "arclight-forge", label: "Arclight (Forge)" },
  { id: "arclight-neoforge", label: "Arclight (NeoForge)" },
  { id: "arclight-fabric", label: "Arclight (Fabric)" },
] as const;

export function ServerInstallPanel({ profileId, remote, rootConfigured, canInstall, onUpdated }: { profileId: string; remote: boolean; rootConfigured: boolean; canInstall: boolean; onUpdated: () => Promise<void> }) {
  const [loader, setLoader] = useState<string>("vanilla");
  const [versions, setVersions] = useState<string[]>([]);
  const [version, setVersion] = useState("");
  const [includeSnapshots, setIncludeSnapshots] = useState(false);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<string | null>(null);
  const [progress, setProgress] = useState<Progress | null>(null);

  useEffect(() => {
    let active = true;
    setLoading(true);
    invoke<string[]>("list_vanilla_versions", { includeSnapshots }).then((items) => {
      if (active) { setVersions(items); setVersion((current) => items.includes(current) ? current : items[0] ?? ""); setLoading(false); }
    }).catch((cause) => {
      if (active) { setError(String(cause)); setLoading(false); }
    });
    return () => { active = false; };
  }, [includeSnapshots]);

  useEffect(() => {
    const listener = listen<Progress>("vanilla-install-progress", (event) => {
      if (event.payload.profileId === profileId) setProgress(event.payload);
    });
    return () => { void listener.then((unlisten) => unlisten()); };
  }, [profileId]);

  async function install(javaOnly = false) {
    setBusy(true);
    setError(null);
    setResult(null);
    setProgress(null);
    try {
      setResult(await invoke<string>(javaOnly ? "use_managed_java" : loader === "vanilla" ? "install_vanilla_version" : "install_server_software", { profileId, versionId: version, loader }));
      await onUpdated();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }

  const phase = progress?.phase === "downloading" && progress.total
    ? `Downloading Minecraft ${Math.round(progress.downloaded / progress.total * 100)}%`
    : progress?.phase === "downloading_remote" ? "Downloading on the remote server…"
    : progress?.phase === "downloading_remote_java" ? "Downloading Java on the remote server…"
    : progress?.phase === "downloading_java" && progress.total
      ? `Downloading Java ${Math.round(progress.downloaded / progress.total * 100)}%`
    : progress?.phase === "uploading_java" ? "Uploading Java to the remote environment…"
    : progress?.phase === "installing_java" ? "Extracting Java…"
    : progress?.phase === "java_installed" ? "Using installed Java…"
    : progress?.phase === "checking_java" ? "Finding a compatible Java version…"
    : progress?.phase === "uploading" ? "Uploading to remote cache…"
    : progress?.phase === "installing_loader" ? "Running the server installer…"
    : progress?.phase === "installing" ? "Copying into server folder…"
    : progress?.phase === "cached" ? "Using cached jar…"
    : progress?.phase === "checking" ? "Checking official version…"
    : "Preparing installation…";

  return <div className="install-panel">
    <div>
      <h2>Install server software</h2>
      <p>Choose a Minecraft version and server type. A compatible Java runtime is shared across servers. Forge, NeoForge, and Quilt require an empty server folder; other installs leave existing files in place.</p>
    </div>
    <div className="install-form">
      <label htmlFor="server-software">Server software</label>
      <Select value={loader} onValueChange={(value) => setLoader(value ?? "vanilla")} disabled={busy}>
        <SelectTrigger id="server-software" className="install-version-trigger"><SelectValue>{(selected) => software.find((item) => item.id === selected)?.label ?? "Choose software"}</SelectValue></SelectTrigger>
        <SelectContent>{software.map((item) => <SelectItem key={item.id} value={item.id}>{item.label}</SelectItem>)}</SelectContent>
      </Select>
      <label htmlFor="vanilla-version">Minecraft version</label>
      <label><input type="checkbox" checked={includeSnapshots} onChange={(event) => setIncludeSnapshots(event.target.checked)} disabled={busy} /> Include snapshots</label>
      <Select value={version} onValueChange={(value) => setVersion(value ?? "")} disabled={loading || busy}>
        <SelectTrigger id="vanilla-version" className="install-version-trigger"><SelectValue>{(selected) => selected || "Choose a version"}</SelectValue></SelectTrigger>
        <SelectContent>{versions.map((item) => <SelectItem key={item} value={item}>{item}</SelectItem>)}</SelectContent>
      </Select>
      <button className="button button-primary" type="button" disabled={loading || busy || !version || !rootConfigured || !canInstall} onClick={() => void install()}>{busy ? "Installing…" : `Install ${software.find((item) => item.id === loader)?.label ?? "software"}`}</button>
      <button className="button button-secondary" type="button" disabled={loading || busy || !version || !rootConfigured || !canInstall} onClick={() => void install(true)}>Use Java for this version</button>
      <small>Already installed? Configure Java without replacing the server jar.</small>
    </div>
    {loading && <p role="status">Loading official releases…</p>}
    {!rootConfigured && <p className="form-error">Set the {remote ? "remote environment" : "local"} Minecraft root on the Servers page first.</p>}
    {rootConfigured && !canInstall && <p className="form-error">Stop the server and confirm its status before installing.</p>}
    {busy && <p role="status">{phase}</p>}
    {error && <p className="form-error" role="alert">{error}</p>}
    {result && <p className="install-success" role="status">{result}</p>}
  </div>;
}
