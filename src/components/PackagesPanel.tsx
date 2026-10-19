import { useEffect, useRef, useState } from "react";
import type { FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Download, Package, RefreshCw, Search } from "lucide-react";
import { serverFiles } from "../lib/serverFiles";
import type { ServerFileEntry } from "../lib/serverFiles";
import { packageLoader } from "../lib/serverSoftware";
import type { PackageKind } from "../lib/serverSoftware";
import { serverProfiles } from "../lib/serverProfiles";
import type { InstalledPackage } from "../lib/serverProfiles";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "./ui/select";

type Project = { id: string; title: string; description: string; author: string; iconUrl: string | null; downloads: number };
type Version = { id: string; name: string; versionNumber: string; requiredDependencies: number };
type PlanEntry = { projectId: string; versionId: string; title: string; versionNumber: string; filename: string; requiredBy: string[]; alreadyInstalled: boolean; direct: boolean };

const loaders: Record<PackageKind, { id: string; label: string }[]> = {
  mod: [{ id: "fabric", label: "Fabric" }, { id: "quilt", label: "Quilt" }, { id: "forge", label: "Forge" }, { id: "neoforge", label: "NeoForge" }],
  plugin: [{ id: "paper", label: "Paper" }, { id: "purpur", label: "Purpur" }, { id: "spigot", label: "Spigot" }, { id: "bukkit", label: "Bukkit" }, { id: "folia", label: "Folia" }],
};

function errorText(error: unknown) { return error instanceof Error ? error.message : String(error); }

function formatSize(size: number) {
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

function minecraftVersion(text: string) {
  return text.match(/(?:Starting minecraft server version|Loading Minecraft)\s+([0-9][a-zA-Z0-9._-]*)/i)?.[1] ?? null;
}

async function detectMinecraftVersion(profileId: string, remote: boolean) {
  try {
    const log = await invoke<string>(remote ? "read_remote_server_log_start" : "read_local_server_log_start", { profileId });
    const version = minecraftVersion(log);
    if (version) return version;
  } catch { /* A server may not have created its log yet. */ }

  if (remote) {
    try {
      const console = await invoke<{ lines: string[] }>("remote_server_console", { profileId });
      const version = minecraftVersion(console.lines.join("\n"));
      if (version) return version;
    } catch { /* Stopped servers have no live console. */ }
    try {
      const version = await invoke<string | null>("detect_remote_fabric_version", { profileId });
      if (version) return version;
    } catch { /* The server may not have a matching managed download. */ }
  }

  for (const path of ["libraries/net/fabricmc/intermediary", ".fabric/remappedJars"]) {
    try {
      const entries = await serverFiles.list(remote, profileId, path);
      const versions = entries.filter((entry) => entry.directory && /^[0-9][a-zA-Z0-9._-]*$/.test(entry.name));
      if (versions.length === 1) return versions[0].name;
    } catch { /* This server may not have that loader directory. */ }
  }

  return null;
}

export function PackagesPanel({ profileId, remote, canInstall, kind, software, installedVersion }: { profileId: string; remote: boolean; canInstall: boolean; kind: PackageKind; software?: string | null; installedVersion?: string | null }) {
  const fixedLoader = packageLoader(kind, software);
  const availableLoaders = fixedLoader ? loaders[kind].filter((item) => item.id === fixedLoader) : loaders[kind];
  const [loader, setLoader] = useState(fixedLoader ?? loaders[kind][0].id);
  const [gameVersion, setGameVersion] = useState(installedVersion ?? "");
  const [versionLoading, setVersionLoading] = useState(!installedVersion);
  const [versionError, setVersionError] = useState<string | null>(null);
  const [versionRetry, setVersionRetry] = useState(0);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState("relevance");
  const [projects, setProjects] = useState<Project[]>([]);
  const [hasSearched, setHasSearched] = useState(false);
  const [selectedProject, setSelectedProject] = useState<Project | null>(null);
  const [versions, setVersions] = useState<Version[]>([]);
  const [selectedVersionId, setSelectedVersionId] = useState("");
  const [installed, setInstalled] = useState<ServerFileEntry[]>([]);
  const [installedPackages, setInstalledPackages] = useState<InstalledPackage[]>([]);
  const [plan, setPlan] = useState<PlanEntry[]>([]);
  const [loadingPlan, setLoadingPlan] = useState(false);
  const [loadingInstalled, setLoadingInstalled] = useState(true);
  const [busy, setBusy] = useState(false);
  const [searching, setSearching] = useState(false);
  const [loadingVersions, setLoadingVersions] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [installedError, setInstalledError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const versionRequest = useRef(0);
  const searchRequest = useRef(0);
  const planRequest = useRef(0);
  const folder = kind === "mod" ? "mods" : "plugins";

  async function refreshInstalled(active = true) {
    setLoadingInstalled(true);
    setInstalledError(null);
    try {
      const [root, profiles] = await Promise.all([serverFiles.list(remote, profileId, ""), serverProfiles.list()]);
      if (!active) return;
      setInstalledPackages(profiles.find((profile) => profile.id === profileId)?.installedPackages ?? []);
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

  useEffect(() => {
    if (installedVersion) {
      setGameVersion(installedVersion);
      setVersionLoading(false);
      setVersionError(null);
      return;
    }
    let active = true;
    setVersionLoading(true);
    setVersionError(null);
    void detectMinecraftVersion(profileId, remote).then((version) => {
      if (!active) return;
      setGameVersion(version ?? "");
      if (!version) setVersionError("Could not determine this server’s Minecraft version from its installed files or console. Start it, then retry while it is running.");
    }).catch((cause) => {
      if (active) { setGameVersion(""); setVersionError(`Could not detect the Minecraft version: ${errorText(cause)}`); }
    }).finally(() => { if (active) setVersionLoading(false); });
    return () => { active = false; };
  }, [profileId, remote, installedVersion, versionRetry]);

  function changeCompatibility(nextLoader: string) {
    setLoader(nextLoader);
    setProjects([]);
    setHasSearched(false);
    versionRequest.current += 1;
    setSelectedProject(null);
    setVersions([]);
    setSelectedVersionId("");
    setPlan([]);
    planRequest.current += 1;
    searchRequest.current += 1;
  }

  async function search(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!gameVersion) return;
    setSearching(true);
    setError(null);
    setMessage(null);
    setSelectedProject(null);
    setVersions([]);
    setProjects([]);
    setHasSearched(false);
    versionRequest.current += 1;
    planRequest.current += 1;
    setPlan([]);
    const request = ++searchRequest.current;
    try {
      const found = await invoke<Project[]>("search_modrinth", { query, kind, gameVersion, loader, index: sort });
      if (request === searchRequest.current) { setProjects(found); setHasSearched(true); }
    } catch (cause) { if (request === searchRequest.current) setError(errorText(cause)); }
    finally { if (request === searchRequest.current) setSearching(false); }
  }

  async function chooseProject(project: Project) {
    const request = ++versionRequest.current;
    setSelectedProject(project);
    setVersions([]);
    setSelectedVersionId("");
    setPlan([]);
    planRequest.current += 1;
    setLoadingVersions(true);
    setError(null);
    try {
      const found = await invoke<Version[]>("list_modrinth_versions", { projectId: project.id, kind, gameVersion, loader });
      if (request === versionRequest.current) {
        setVersions(found);
        setSelectedVersionId(found[0]?.id ?? "");
      }
    } catch (cause) { if (request === versionRequest.current) setError(errorText(cause)); }
    finally { if (request === versionRequest.current) setLoadingVersions(false); }
  }

  async function install() {
    if (!selectedProject || !selectedVersionId || plan.length === 0 || loadingPlan) return;
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      const result = await invoke<string>("install_modrinth_plan", { profileId, kind, gameVersion, loader, projectId: selectedProject.id, versionId: selectedVersionId, expectedVersions: plan.map((item) => item.versionId) });
      setMessage(result);
      await refreshInstalled();
      setPlan([]);
    } catch (cause) { setError(errorText(cause)); await refreshInstalled(); }
    finally { setBusy(false); }
  }

  async function removePlugin(record: InstalledPackage) {
    if (!window.confirm(`Remove ${record.title} (${record.filename}) from this server?`)) return;
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      await invoke("remove_modrinth_plugin", { profileId, projectId: record.projectId, versionId: record.versionId });
      setMessage(`Removed ${record.title}.`);
      await refreshInstalled();
      setPlan([]);
    } catch (cause) { setError(errorText(cause)); await refreshInstalled(); }
    finally { setBusy(false); }
  }

  const selectedVersion = versions.find((version) => version.id === selectedVersionId);
  const selectedInstalled = installedPackages.find((item) => item.kind === kind && item.projectId === selectedProject?.id && installed.some((file) => file.name === item.filename));
  const updating = kind === "plugin" && selectedInstalled && selectedInstalled.versionId !== selectedVersionId;
  const dependencyFiles = installed.filter((entry) => installedPackages.some((item) => item.kind === kind && item.filename === entry.name && !item.direct && item.requiredBy.length > 0));
  const primaryFiles = installed.filter((entry) => !dependencyFiles.includes(entry));

  function installedRow(entry: ServerFileEntry) {
    const record = installedPackages.find((item) => item.kind === kind && item.filename === entry.name);
    return <li key={entry.path}><Package size={16} aria-hidden="true" /><span className="packages-installed-copy" title={entry.name}><strong>{record?.title ?? entry.name}</strong>{record && <small>{entry.name}</small>}{record?.requiredBy.length ? <em className="packages-tag packages-tag-dependency">Dependency for {record.requiredBy.join(", ")}</em> : null}</span><small>{formatSize(entry.size)}</small>{kind === "plugin" && record && <span className="packages-installed-actions"><button type="button" className="button button-quiet" disabled={busy || !gameVersion || record.requiredBy.length > 0} onClick={() => void chooseProject({ id: record.projectId, title: record.title, description: "", author: "", iconUrl: null, downloads: 0 })}>Update</button><button type="button" className="button button-quiet" disabled={busy || !canInstall || record.requiredBy.length > 0} onClick={() => void removePlugin(record)}>Remove</button></span>}</li>;
  }

  useEffect(() => {
    if (!selectedProject || !selectedVersionId || !gameVersion) return;
    const request = ++planRequest.current;
    setPlan([]);
    setLoadingPlan(true);
    setError(null);
    void invoke<PlanEntry[]>("preview_modrinth_install", { profileId, kind, gameVersion, loader, projectId: selectedProject.id, versionId: selectedVersionId })
      .then((items) => { if (request === planRequest.current) setPlan(items); })
      .catch((cause) => { if (request === planRequest.current) setError(errorText(cause)); })
      .finally(() => { if (request === planRequest.current) setLoadingPlan(false); });
    return () => { planRequest.current += 1; };
  }, [profileId, kind, gameVersion, loader, selectedProject?.id, selectedVersionId]);

  return <section className="packages-panel" aria-label={kind === "mod" ? "Mods" : "Plugins"}>
    <header className="packages-header">
      <div><h2>{kind === "mod" ? "Mods" : "Plugins"}</h2><p>Browse compatible Modrinth releases and install them into this server.</p></div>
    </header>
    <div className="packages-layout">
      <div className="packages-catalog">
        <h3>Find {kind === "mod" ? "a mod" : "a plugin"}</h3>
        <p>Results are filtered for this server’s Minecraft version and loader.</p>
        <form className="packages-search-form" onSubmit={(event) => void search(event)}>
          <div className="packages-fixed-loader"><span>Minecraft version</span><strong>{versionLoading ? "Detecting…" : gameVersion || "Unknown"}</strong></div>
          {versionError && <p className="files-error" role="alert">{versionError} <button type="button" className="button button-quiet" onClick={() => setVersionRetry((value) => value + 1)}>Retry</button></p>}
          {fixedLoader ? <div className="packages-fixed-loader"><span>Server loader</span><strong>{availableLoaders[0]?.label ?? fixedLoader}</strong></div> : <>
            <label htmlFor="package-loader">Server loader</label>
            <Select value={loader} onValueChange={(value) => changeCompatibility(value ?? availableLoaders[0].id)} disabled={busy}>
              <SelectTrigger id="package-loader"><SelectValue>{(value) => loaders[kind].find((item) => item.id === value)?.label ?? "Choose loader"}</SelectValue></SelectTrigger>
              <SelectContent>{availableLoaders.map((item) => <SelectItem key={item.id} value={item.id}>{item.label}</SelectItem>)}</SelectContent>
            </Select>
          </>}
          <label htmlFor="package-query">Search Modrinth</label>
          <div className="packages-query"><input id="package-query" value={query} onChange={(event) => setQuery(event.target.value)} placeholder={kind === "mod" ? "Search mods or browse popular" : "Search plugins or browse popular"} maxLength={100} disabled={busy} /><button className="button button-primary" type="submit" disabled={searching || busy || !gameVersion}><Search size={15} aria-hidden="true" /> {searching ? "Searching…" : query ? "Search" : "Browse"}</button></div>
          <div className="packages-sort"><label htmlFor="package-sort">Sort results</label><Select value={sort} onValueChange={(value) => setSort(value ?? "relevance")} disabled={busy}><SelectTrigger id="package-sort"><SelectValue>{(value) => ({ relevance: "Relevance", downloads: "Most downloaded", updated: "Recently updated" })[value as "relevance" | "downloads" | "updated"] ?? "Relevance"}</SelectValue></SelectTrigger><SelectContent><SelectItem value="relevance">Relevance</SelectItem><SelectItem value="downloads">Most downloaded</SelectItem><SelectItem value="updated">Recently updated</SelectItem></SelectContent></Select></div>
        </form>
        {error && <p className="files-error" role="alert">{error}</p>}
        {message && <p className="install-success" role="status">{message}</p>}
        {(projects.length > 0 || selectedProject) && <div className="packages-browser">
          <div className="packages-results" aria-label="Search results">{projects.map((project) => <button key={project.id} type="button" className={`packages-result${selectedProject?.id === project.id ? " packages-result-selected" : ""}`} onClick={() => void chooseProject(project)} disabled={busy}>
            <span className="packages-result-icon">{project.iconUrl ? <img src={project.iconUrl} alt="" loading="lazy" /> : <Package size={20} aria-hidden="true" />}</span>
            <span className="packages-result-copy"><strong>{project.title}</strong><span>{project.description}</span><small>by {project.author} · {Intl.NumberFormat(undefined, { notation: "compact" }).format(project.downloads)} downloads</small>{installedPackages.some((item) => item.kind === kind && item.projectId === project.id && installed.some((file) => file.name === item.filename)) && <em>Installed</em>}</span>
          </button>)}</div>
          <div className="packages-version">
            {selectedProject ? <>
              <div className="packages-detail-heading">{selectedProject.iconUrl && <img src={selectedProject.iconUrl} alt="" />}<div><h3>{selectedProject.title}</h3>{selectedProject.author && <p>by {selectedProject.author}</p>}</div></div>
              <p>{selectedProject.description}</p>
              {loadingVersions ? <p role="status">Loading compatible releases…</p> : versions.length === 0 ? <p>No compatible releases were found.</p> : <>
                <label htmlFor="package-version">Release</label>
                <Select value={selectedVersionId} onValueChange={(value) => setSelectedVersionId(value ?? "")} disabled={busy}>
                  <SelectTrigger id="package-version"><SelectValue>{(value) => versions.find((version) => version.id === value)?.name ?? "Choose release"}</SelectValue></SelectTrigger>
                  <SelectContent>{versions.map((version) => <SelectItem key={version.id} value={version.id}>{version.name || version.versionNumber}</SelectItem>)}</SelectContent>
                </Select>
                {selectedVersion && <p>{selectedVersion.versionNumber}</p>}
                {updating && <p>The new JAR is verified before the previous version is removed. If cleanup fails, both JARs may remain; review the installed list before starting the server.</p>}
                <div className="packages-plan" aria-live="polite">
                  <h4>Review download</h4>
                  {loadingPlan ? <p>Finding required dependencies…</p> : plan.length > 0 ? <>
                    <p>{plan.filter((item) => !item.alreadyInstalled).length} JARs to add · {plan.filter((item) => item.alreadyInstalled).length} already installed{updating ? ` · replaces ${selectedInstalled.filename}` : ""}</p>
                    <ul>{[...plan].reverse().map((item) => <li key={item.projectId}><div><strong>{item.title}</strong><small>{item.versionNumber} · {item.filename}</small></div><span className={item.direct ? "packages-tag" : "packages-tag packages-tag-dependency"}>{item.direct ? `Selected ${kind}` : `Dependency for ${item.requiredBy.join(", ")}`}</span>{item.alreadyInstalled && <span className="packages-tag packages-tag-installed">Installed</span>}</li>)}</ul>
                  </> : null}
                </div>
                <button className="button button-primary" type="button" onClick={() => void install()} disabled={!canInstall || busy || loadingPlan || plan.length === 0}><Download size={15} aria-hidden="true" /> {busy ? "Installing…" : updating ? "Confirm plugin update" : `Confirm and install ${plan.filter((item) => !item.alreadyInstalled).length}`}</button>
                {!canInstall && <p className="files-error">Stop the server and confirm its status before installing.</p>}
              </>}
            </> : <p className="state-message">Select a result to see releases and required dependencies.</p>}
          </div>
        </div>}
        {hasSearched && projects.length === 0 && <p className="state-message">No matching {kind === "mod" ? "mods" : "plugins"} found for this server. Try another search or sort order.</p>}
      </div>
      <aside className="packages-installed" aria-label={`Installed ${folder}`}>
        <div className="packages-installed-heading"><div><h3>Installed {folder}</h3><p>JAR files in this server’s {folder}/ folder</p></div><button type="button" className="button button-quiet" onClick={() => void refreshInstalled()} disabled={loadingInstalled}><RefreshCw size={14} aria-hidden="true" /> Refresh</button></div>
        {installedError && <p className="files-error" role="alert">{installedError}</p>}
        {loadingInstalled ? <p className="state-message">Loading {folder}…</p> : installed.length === 0 ? <p className="state-message">No {folder} installed yet.</p> : <>
          {primaryFiles.length > 0 && <ul className="packages-installed-list">{primaryFiles.map(installedRow)}</ul>}
          {dependencyFiles.length > 0 && <section className="packages-installed-dependencies" aria-label={`Installed ${kind} dependencies`}><h4>Dependencies</h4><ul className="packages-installed-list">{dependencyFiles.map(installedRow)}</ul></section>}
        </>}
      </aside>
    </div>
  </section>;
}
