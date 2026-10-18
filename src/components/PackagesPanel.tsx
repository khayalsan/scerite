import { useEffect, useRef, useState } from "react";
import type { FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Download, Package, RefreshCw, Search } from "lucide-react";
import { serverFiles } from "../lib/serverFiles";
import type { ServerFileEntry } from "../lib/serverFiles";
import { packageLoader } from "../lib/serverSoftware";
import type { PackageKind } from "../lib/serverSoftware";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "./ui/select";

type Project = { id: string; title: string; description: string; author: string };
type Version = { id: string; name: string; versionNumber: string; requiredDependencies: number };

const loaders: Record<PackageKind, { id: string; label: string }[]> = {
  mod: [{ id: "fabric", label: "Fabric" }, { id: "quilt", label: "Quilt" }, { id: "forge", label: "Forge" }, { id: "neoforge", label: "NeoForge" }],
  plugin: [{ id: "paper", label: "Paper" }, { id: "purpur", label: "Purpur" }, { id: "spigot", label: "Spigot" }, { id: "bukkit", label: "Bukkit" }, { id: "folia", label: "Folia" }],
};

function errorText(error: unknown) { return error instanceof Error ? error.message : String(error); }

export function PackagesPanel({ profileId, remote, canInstall, kind, software }: { profileId: string; remote: boolean; canInstall: boolean; kind: PackageKind; software?: string | null }) {
  const fixedLoader = packageLoader(kind, software);
  const availableLoaders = fixedLoader ? loaders[kind].filter((item) => item.id === fixedLoader) : loaders[kind];
  const [loader, setLoader] = useState(fixedLoader ?? loaders[kind][0].id);
  const [gameVersion, setGameVersion] = useState("");
  const [query, setQuery] = useState("");
  const [projects, setProjects] = useState<Project[]>([]);
  const [hasSearched, setHasSearched] = useState(false);
  const [selectedProject, setSelectedProject] = useState<Project | null>(null);
  const [versions, setVersions] = useState<Version[]>([]);
  const [selectedVersionId, setSelectedVersionId] = useState("");
  const [installed, setInstalled] = useState<ServerFileEntry[]>([]);
  const [loadingInstalled, setLoadingInstalled] = useState(true);
  const [busy, setBusy] = useState(false);
  const [searching, setSearching] = useState(false);
  const [loadingVersions, setLoadingVersions] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [installedError, setInstalledError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const versionRequest = useRef(0);
  const folder = kind === "mod" ? "mods" : "plugins";

  async function refreshInstalled(active = true) {
    setLoadingInstalled(true);
    setInstalledError(null);
    try {
      const root = await serverFiles.list(remote, profileId, "");
      if (!active) return;
      if (!root.some((entry) => entry.directory && entry.name === folder)) { setInstalled([]); return; }
      const entries = await serverFiles.list(remote, profileId, folder);
      if (active) setInstalled(entries.filter((entry) => !entry.directory && entry.name.toLowerCase().endsWith(".jar")));
    } catch (cause) {
      if (active) setInstalledError(errorText(cause));
    } finally {
      if (active) setLoadingInstalled(false);
    }
  }

  useEffect(() => {
    let active = true;
    void refreshInstalled(active);
    return () => { active = false; };
  }, [profileId, remote, folder]);

  function changeCompatibility(nextVersion: string, nextLoader: string) {
    setGameVersion(nextVersion);
    setLoader(nextLoader);
    setProjects([]);
    setHasSearched(false);
    versionRequest.current += 1;
    setSelectedProject(null);
    setVersions([]);
    setSelectedVersionId("");
  }

  async function search(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setSearching(true);
    setError(null);
    setMessage(null);
    setSelectedProject(null);
    setVersions([]);
    setProjects([]);
    setHasSearched(false);
    versionRequest.current += 1;
    try {
      setProjects(await invoke<Project[]>("search_modrinth", { query, kind, gameVersion: gameVersion.trim(), loader }));
      setHasSearched(true);
    } catch (cause) { setError(errorText(cause)); }
    finally { setSearching(false); }
  }

  async function chooseProject(project: Project) {
    const request = ++versionRequest.current;
    setSelectedProject(project);
    setVersions([]);
    setSelectedVersionId("");
    setLoadingVersions(true);
    setError(null);
    try {
      const found = await invoke<Version[]>("list_modrinth_versions", { projectId: project.id, kind, gameVersion: gameVersion.trim(), loader });
      if (request === versionRequest.current) {
        setVersions(found);
        setSelectedVersionId(found[0]?.id ?? "");
      }
    } catch (cause) { if (request === versionRequest.current) setError(errorText(cause)); }
    finally { if (request === versionRequest.current) setLoadingVersions(false); }
  }

  async function install() {
    if (!selectedProject || !selectedVersionId) return;
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      const result = await invoke<string>("install_modrinth_version", { profileId, kind, gameVersion: gameVersion.trim(), loader, projectId: selectedProject.id, versionId: selectedVersionId });
      setMessage(result);
      await refreshInstalled();
    } catch (cause) { setError(errorText(cause)); }
    finally { setBusy(false); }
  }

  const selectedVersion = versions.find((version) => version.id === selectedVersionId);

  return <section className="packages-panel" aria-label={kind === "mod" ? "Mods" : "Plugins"}>
    <header className="packages-header">
      <div><h2>{kind === "mod" ? "Mods" : "Plugins"}</h2><p>Browse compatible Modrinth releases and install them into this server.</p></div>
    </header>
    <div className="packages-layout">
      <div className="packages-catalog">
        <h3>Find {kind === "mod" ? "a mod" : "a plugin"}</h3>
        <p>Set the Minecraft version and loader this server uses. Results are filtered to server compatible releases.</p>
        <form className="packages-search-form" onSubmit={(event) => void search(event)}>
          <label htmlFor="package-game-version">Minecraft version</label>
          <input id="package-game-version" value={gameVersion} onChange={(event) => changeCompatibility(event.target.value, loader)} placeholder="e.g. 1.21.11" required disabled={busy} />
          {fixedLoader ? <div className="packages-fixed-loader"><span>Server loader</span><strong>{availableLoaders[0]?.label ?? fixedLoader}</strong></div> : <>
            <label htmlFor="package-loader">Server loader</label>
            <Select value={loader} onValueChange={(value) => changeCompatibility(gameVersion, value ?? availableLoaders[0].id)} disabled={busy}>
              <SelectTrigger id="package-loader"><SelectValue>{(value) => loaders[kind].find((item) => item.id === value)?.label ?? "Choose loader"}</SelectValue></SelectTrigger>
              <SelectContent>{availableLoaders.map((item) => <SelectItem key={item.id} value={item.id}>{item.label}</SelectItem>)}</SelectContent>
            </Select>
          </>}
          <label htmlFor="package-query">Search Modrinth</label>
          <div className="packages-query"><input id="package-query" value={query} onChange={(event) => setQuery(event.target.value)} placeholder={kind === "mod" ? "Search mods" : "Search plugins"} minLength={2} maxLength={100} required disabled={busy} /><button className="button button-primary" type="submit" disabled={searching || busy}><Search size={15} aria-hidden="true" /> {searching ? "Searching…" : "Search"}</button></div>
        </form>
        {error && <p className="files-error" role="alert">{error}</p>}
        {message && <p className="install-success" role="status">{message}</p>}
        {projects.length > 0 && <div className="packages-results" aria-label="Search results">{projects.map((project) => <button key={project.id} type="button" className={`packages-result${selectedProject?.id === project.id ? " packages-result-selected" : ""}`} onClick={() => void chooseProject(project)} disabled={busy}><strong>{project.title}</strong><span>{project.description}</span><small>by {project.author}</small></button>)}</div>}
        {hasSearched && projects.length === 0 && <p className="state-message">No matching {kind === "mod" ? "mods" : "plugins"} found. Try another search or version.</p>}
        {selectedProject && <div className="packages-version">
          <h3>{selectedProject.title}</h3>
          {loadingVersions ? <p role="status">Loading compatible releases…</p> : versions.length === 0 ? <p>No compatible releases were found.</p> : <>
            <label htmlFor="package-version">Release</label>
            <Select value={selectedVersionId} onValueChange={(value) => setSelectedVersionId(value ?? "")} disabled={busy}>
              <SelectTrigger id="package-version"><SelectValue>{(value) => versions.find((version) => version.id === value)?.name ?? "Choose release"}</SelectValue></SelectTrigger>
              <SelectContent>{versions.map((version) => <SelectItem key={version.id} value={version.id}>{version.name || version.versionNumber}</SelectItem>)}</SelectContent>
            </Select>
            {selectedVersion && <p>{selectedVersion.versionNumber}{selectedVersion.requiredDependencies > 0 && ` · ${selectedVersion.requiredDependencies} required dependencies to install separately`}</p>}
            <button className="button button-primary" type="button" onClick={() => void install()} disabled={!canInstall || busy || !selectedVersionId}><Download size={15} aria-hidden="true" /> {busy ? "Installing…" : `Install ${kind}`}</button>
            {!canInstall && <p className="files-error">Stop the server and confirm its status before installing.</p>}
          </>}
        </div>}
      </div>
      <aside className="packages-installed" aria-label={`Installed ${folder}`}>
        <div className="packages-installed-heading"><div><h3>Installed {folder}</h3><p>JAR files in this server’s {folder}/ folder</p></div><button type="button" className="button button-quiet" onClick={() => void refreshInstalled()} disabled={loadingInstalled}><RefreshCw size={14} aria-hidden="true" /> Refresh</button></div>
        {installedError && <p className="files-error" role="alert">{installedError}</p>}
        {loadingInstalled ? <p className="state-message">Loading {folder}…</p> : installed.length === 0 ? <p className="state-message">No {folder} installed yet.</p> : <ul className="packages-installed-list">{installed.map((entry) => <li key={entry.path}><Package size={16} aria-hidden="true" /><span title={entry.name}>{entry.name}</span><small>{(entry.size / 1024 / 1024).toFixed(1)} MB</small></li>)}</ul>}
      </aside>
    </div>
  </section>;
}
