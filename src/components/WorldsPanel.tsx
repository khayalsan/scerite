import { useEffect, useRef, useState } from "react";
import { Download, File, Folder, Globe2, RefreshCw, RotateCcw, Upload, Archive } from "lucide-react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { serverFiles } from "../lib/serverFiles";
import type { ServerFileEntry } from "../lib/serverFiles";
import { serverWorlds } from "../lib/serverWorlds";
import type { WorldBackup, WorldSettings } from "../lib/serverWorlds";
import { serverSessions } from "../lib/serverSessions";
import { SearchField } from "./SearchField";

interface WorldsPanelProps {
  profileId: string;
  remote: boolean;
  canEdit: boolean;
  running: boolean;
  backupRevision: number;
}

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function worldName(properties: string) {
  const setting = properties.split(/\r?\n/).find((line) => /^\s*level-name\s*=/.test(line));
  return setting?.slice(setting.indexOf("=") + 1).trim() || "world";
}

function property(properties: string, key: string) {
  return properties.split(/\r?\n/).find((line) => line.trimStart().startsWith(`${key}=`))?.split("=").slice(1).join("=").trim() ?? "";
}

function validRuleValue(value: string) {
  return /^(?:true|false|-?\d{1,10})$/.test(value) &&
    (!/^-?\d/.test(value) || (Number.isSafeInteger(Number(value)) && Math.abs(Number(value)) <= 2147483647));
}

export function WorldsPanel({ profileId, remote, canEdit, running, backupRevision }: WorldsPanelProps) {
  const [worlds, setWorlds] = useState<ServerFileEntry[]>([]);
  const [backups, setBackups] = useState<WorldBackup[]>([]);
  const [backupsLoading, setBackupsLoading] = useState(true);
  const [backupsError, setBackupsError] = useState<string | null>(null);
  const [autoBackupEnabled, setAutoBackupEnabled] = useState(false);
  const [autoBackupKeep, setAutoBackupKeep] = useState("3");
  const [autoBackupLoading, setAutoBackupLoading] = useState(true);
  const [autoBackupSaving, setAutoBackupSaving] = useState(false);
  const [autoBackupError, setAutoBackupError] = useState<string | null>(null);
  const [activeWorldName, setActiveWorldName] = useState("");
  const [selectedWorld, setSelectedWorld] = useState<string | null>(null);
  const [directory, setDirectory] = useState<string | null>(null);
  const [entries, setEntries] = useState<ServerFileEntry[]>([]);
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isLoadingDirectory, setIsLoadingDirectory] = useState(false);
  const [revision, setRevision] = useState(0);
  const [settings, setSettings] = useState<WorldSettings | null>(null);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [settingsLoading, setSettingsLoading] = useState(false);
  const [seed, setSeed] = useState("");
  const [levelType, setLevelType] = useState("minecraft:normal");
  const [structures, setStructures] = useState(true);
  const [ruleValues, setRuleValues] = useState<Record<string, string>>({});
  const [ruleSearch, setRuleSearch] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const previewRequest = useRef(0);

  useEffect(() => {
    let active = true;
    setIsLoading(true);
    setError(null);
    setWorlds([]);
    setActiveWorldName("");
    void (async () => {
      const found = await serverFiles.worlds(remote, profileId);
      if (!active) return;
      setWorlds(found);
      setIsLoading(false);
      const properties = await serverFiles.read(remote, profileId, "server.properties").catch(() => "");
      if (active) {
        setActiveWorldName(worldName(properties));
        setSeed(property(properties, "level-seed"));
        setLevelType(property(properties, "level-type") || "minecraft:normal");
        setStructures(property(properties, "generate-structures") !== "false");
      }
    })().catch((reason: unknown) => { if (active) setError(errorText(reason)); })
      .finally(() => { if (active) setIsLoading(false); });
    return () => { active = false; };
  }, [profileId, remote, revision]);

  useEffect(() => {
    let active = true;
    setBackupsLoading(true);
    setBackupsError(null);
    void serverWorlds.backups(remote, profileId)
      .then((found) => { if (active) setBackups(found); })
      .catch((reason: unknown) => { if (active) setBackupsError(errorText(reason)); })
      .finally(() => { if (active) setBackupsLoading(false); });
    return () => { active = false; };
  }, [profileId, remote, revision, backupRevision]);

  useEffect(() => {
    let active = true;
    setAutoBackupLoading(true);
    void serverWorlds.autoBackupSettings(profileId)
      .then((settings) => {
        if (active) { setAutoBackupEnabled(settings.enabled); setAutoBackupKeep(String(settings.keep)); }
      })
      .catch((reason: unknown) => { if (active) setAutoBackupError(errorText(reason)); })
      .finally(() => { if (active) setAutoBackupLoading(false); });
    return () => { active = false; };
  }, [profileId]);

  useEffect(() => {
    if (!selectedWorld) { setSettings(null); return; }
    let active = true;
    setSettings(null);
    setSettingsError(null);
    setSettingsLoading(true);
    void serverWorlds.settings(remote, profileId, selectedWorld).then((result) => {
      if (active) { setSettings(result); setRuleValues(Object.fromEntries(result.gamerules.map((rule) => [rule.name, rule.value]))); }
    }).catch((reason: unknown) => {
      if (active) setSettingsError(errorText(reason));
    }).finally(() => { if (active) setSettingsLoading(false); });
    return () => { active = false; };
  }, [profileId, remote, selectedWorld, revision]);

  useEffect(() => {
    if (!directory) return;
    let active = true;
    setIsLoadingDirectory(true);
    setEntries([]);
    setError(null);
    void serverFiles.list(remote, profileId, directory)
      .then((files) => { if (active) setEntries(files); })
      .catch((reason: unknown) => { if (active) setError(errorText(reason)); })
      .finally(() => { if (active) setIsLoadingDirectory(false); });
    return () => { active = false; };
  }, [directory, profileId, remote, revision]);

  function openWorld(path: string) {
    previewRequest.current += 1;
    setSelectedWorld(path);
    setDirectory(path);
    setSelectedFile(null);
    setPreview(null);
    setPreviewError(null);
    setMessage(null);
    setRuleSearch("");
  }

  async function backupWorld() {
    if (!selectedWorld || !canEdit) return;
    setBusy("backup"); setError(null); setMessage(null);
    try {
      const path = await serverWorlds.backup(remote, profileId, selectedWorld);
      setMessage(`Backup saved to ${path}`);
      setRevision((value) => value + 1);
    } catch (reason) { setError(`Could not back up the world: ${errorText(reason)}`); }
    finally { setBusy(null); }
  }

  async function downloadWorld() {
    if (!selectedWorld || !canEdit) return;
    const destination = await saveDialog({ defaultPath: `${selectedWorld}.zip`, filters: [{ name: "ZIP archive", extensions: ["zip"] }] });
    if (!destination) return;
    setBusy("download"); setError(null); setMessage(null);
    try {
      await serverWorlds.download(remote, profileId, selectedWorld, destination);
      setMessage(`World ZIP saved to ${destination}`);
    } catch (reason) { setError(`Could not download the world: ${errorText(reason)}`); }
    finally { setBusy(null); }
  }

  async function uploadWorld() {
    if (!selectedWorld || !canEdit) return;
    const source = await openDialog({ multiple: false, filters: [{ name: "ZIP archive", extensions: ["zip"] }] });
    if (typeof source !== "string") return;
    if (!window.confirm(`Replace ${selectedWorld} with this ZIP? Scerite will keep the current world in the server's .scerite-world-backups folder.`)) return;
    setBusy("upload"); setError(null); setMessage(null);
    try {
      const preserved = await serverWorlds.upload(remote, profileId, selectedWorld, source);
      setMessage(`World uploaded. The previous world is preserved at ${preserved}`);
      setRevision((value) => value + 1);
    } catch (reason) { setError(`Could not upload the world: ${errorText(reason)}`); }
    finally { setBusy(null); }
  }

  async function regenerateWorld() {
    if (!selectedWorld || !canEdit || selectedWorld !== activeWorldName) return;
    if (!levelType.trim() || /[\r\n]/.test(seed + levelType)) { setError("Enter a valid seed and world type."); return; }
    if (!window.confirm(`Generate ${selectedWorld} again on the next server start? The current world will be kept in the server's .scerite-world-backups folder.`)) return;
    setBusy("regenerate"); setError(null); setMessage(null);
    try {
      const preserved = await serverWorlds.regenerate(remote, profileId, selectedWorld, seed, levelType.trim(), structures);
      setSelectedWorld(null); setDirectory(null);
      setMessage(`The current world is preserved at ${preserved}. Start the server to generate a new world.`);
      setRevision((value) => value + 1);
    } catch (reason) { setError(`Could not prepare world generation: ${errorText(reason)}`); }
    finally { setBusy(null); }
  }

  async function restoreBackup(backup: WorldBackup) {
    if (!canEdit || busy) return;
    const label = backup.modified ? new Date(backup.modified * 1000).toLocaleString() : backup.path;
    if (!window.confirm(`Restore ${backup.name} from the ${backup.kind === "zip" ? "ZIP backup" : "preserved world"} saved ${label}? Scerite will preserve the current world first. Keep the server stopped until restoration finishes.`)) return;
    setBusy("restore"); setError(null); setMessage(null);
    try {
      const preserved = await serverWorlds.restore(remote, profileId, backup);
      setMessage(preserved ? `World restored. The previous world is preserved at ${preserved}` : `World ${backup.name} restored.`);
      setRevision((value) => value + 1);
    } catch (reason) { setError(`Could not restore ${backup.name}: ${errorText(reason)}`); }
    finally { setBusy(null); }
  }

  async function saveAutoBackupSettings() {
    const keep = Number(autoBackupKeep);
    if (!Number.isInteger(keep) || keep < 1 || keep > 100) {
      setAutoBackupError("Keep between 1 and 100 automatic backups."); return;
    }
    setAutoBackupSaving(true); setAutoBackupError(null);
    try {
      const saved = await serverWorlds.saveAutoBackupSettings(profileId, autoBackupEnabled, keep);
      setAutoBackupEnabled(saved.enabled); setAutoBackupKeep(String(saved.keep));
      setMessage("Automatic backup settings saved.");
    } catch (reason) { setAutoBackupError(errorText(reason)); }
    finally { setAutoBackupSaving(false); }
  }

  async function deleteBackup(backup: WorldBackup) {
    if (backup.kind !== "zip" || busy) return;
    if (!window.confirm(`Delete this ${backup.automatic ? "automatic" : "manual"} ZIP backup of ${backup.name}? This cannot be undone.`)) return;
    setBusy("delete"); setError(null); setMessage(null);
    try {
      await serverWorlds.deleteZipBackup(profileId, backup);
      setMessage(`Deleted the ZIP backup of ${backup.name}.`);
      setRevision((value) => value + 1);
    } catch (reason) { setError(`Could not delete the backup: ${errorText(reason)}`); }
    finally { setBusy(null); }
  }

  async function applyRule(name: string) {
    const value = ruleValues[name]?.trim() ?? "";
    if (!running || !/^(?:[A-Za-z0-9_]{1,64}|[a-z0-9_.-]+:[a-z0-9_/.-]+)$/.test(name) || !validRuleValue(value)) {
      setError("Enter a true/false value or a whole number for this gamerule."); return;
    }
    setBusy(name); setError(null); setMessage(null);
    try {
      if (remote) await serverSessions.sendRemoteInput(profileId, `gamerule ${name} ${value}`);
      else await serverSessions.sendInput(profileId, `gamerule ${name} ${value}`);
      setMessage(`Sent gamerule ${name} ${value}. Check the console for the result; refresh after Minecraft saves the world.`);
    } catch (reason) { setError(`Could not update ${name}: ${errorText(reason)}`); }
    finally { setBusy(null); }
  }

  function openDirectory(path: string) {
    previewRequest.current += 1;
    setDirectory(path);
    setSelectedFile(null);
    setPreview(null);
    setPreviewError(null);
  }

  async function openFile(entry: ServerFileEntry) {
    if (entry.directory) {
      openDirectory(entry.path);
      return;
    }
    const request = ++previewRequest.current;
    setSelectedFile(entry.path);
    setPreview(null);
    setPreviewError(null);
    try {
      const content = await serverFiles.read(remote, profileId, entry.path);
      if (request === previewRequest.current) setPreview(content);
    } catch (reason) {
      if (request === previewRequest.current) setPreviewError(errorText(reason));
    }
  }

  const parent = directory?.slice(0, directory.lastIndexOf("/")) ?? "";
  const backupGroups = [...new Set(backups.map((backup) => backup.name))]
    .map((name) => ({ name, entries: backups.filter((backup) => backup.name === name) }));

  return (
    <section className="worlds-panel" aria-label="World browser">
      <header className="worlds-header">
        <div>
          <h2>Worlds</h2>
          <p>Browse world files, settings, backups, and ZIP transfers.</p>
        </div>
        <button className="button button-quiet" type="button" onClick={() => setRevision((value) => value + 1)} disabled={isLoading}>
          <RefreshCw size={14} aria-hidden="true" /> Refresh
        </button>
      </header>
      {error && <p className="files-error" role="alert">{error}</p>}
      {message && <p className="notice" role="status">{message}</p>}
      <section className="worlds-auto-backup" aria-label="Automatic world backups">
        <div><h4>Automatic backups</h4><p>While Scerite is open, back up the active world after the server stops. Only automatic ZIPs are removed when the limit is reached.</p></div>
        <label className="worlds-auto-toggle"><input type="checkbox" checked={autoBackupEnabled} onChange={(event) => setAutoBackupEnabled(event.currentTarget.checked)} disabled={autoBackupLoading || autoBackupSaving} /> Back up when the server stops</label>
        <label className="worlds-auto-keep">Automatic backups to keep<input type="number" min="1" max="100" step="1" value={autoBackupKeep} onChange={(event) => setAutoBackupKeep(event.currentTarget.value)} disabled={autoBackupLoading || autoBackupSaving} /></label>
        <button className="button button-secondary" type="button" disabled={autoBackupLoading || autoBackupSaving} onClick={() => void saveAutoBackupSettings()}>{autoBackupSaving ? "Saving…" : "Save settings"}</button>
        {autoBackupError && <p className="files-error" role="alert">{autoBackupError}</p>}
      </section>
      {isLoading ? <p className="state-message">Finding worlds…</p> : worlds.length === 0 ? (
        <p className="state-message">No world folders were found. A world appears here after the server creates a folder containing level.dat.</p>
      ) : (
        <div className="worlds-list">
          {worlds.map((world) => (
            <button key={world.path} className={`world-card${selectedWorld === world.path ? " world-card-selected" : ""}`} type="button" onClick={() => openWorld(world.path)}>
              <Globe2 size={20} aria-hidden="true" />
              <span><strong>{world.name}</strong><small>{world.name === activeWorldName ? "Active world" : "World folder"}</small></span>
            </button>
          ))}
        </div>
      )}
      {selectedWorld && <div className="worlds-settings">
        <div className="worlds-settings-heading"><h3>{selectedWorld}</h3><span>{selectedWorld === activeWorldName ? "Active world" : "Stored world"}</span></div>
        <div className="worlds-settings-actions">
          <button className="button button-secondary" type="button" disabled={!canEdit || !!busy} onClick={() => void backupWorld()}><Archive size={15} aria-hidden="true" /> {busy === "backup" ? "Backing up…" : "Back up"}</button>
          <button className="button button-secondary" type="button" disabled={!canEdit || !!busy} onClick={() => void downloadWorld()}><Download size={15} aria-hidden="true" /> {busy === "download" ? "Downloading…" : "Download ZIP"}</button>
          <button className="button button-secondary" type="button" disabled={!canEdit || !!busy} onClick={() => void uploadWorld()}><Upload size={15} aria-hidden="true" /> {busy === "upload" ? "Uploading…" : "Upload ZIP"}</button>
        </div>
        <p className="players-caption">{canEdit ? "Keep the server stopped until a world operation finishes." : "Stop the server before backing up, downloading, uploading, or generating a world again."}</p>
        {selectedWorld === activeWorldName && <section className="worlds-generate" aria-label="Generate world again">
          <div><h4>Generate again</h4><p>These settings apply on the next server start. The current world and any matching Nether and End folders are preserved on the server.</p></div>
          <div className="worlds-generation-fields">
            <label>New seed<input value={seed} maxLength={128} onChange={(event) => setSeed(event.currentTarget.value)} disabled={!canEdit || !!busy} placeholder="Leave blank for a random seed" /></label>
            <label>World type<input value={levelType} maxLength={128} onChange={(event) => setLevelType(event.currentTarget.value)} disabled={!canEdit || !!busy} /></label>
            <label className="worlds-checkbox"><input type="checkbox" checked={structures} onChange={(event) => setStructures(event.currentTarget.checked)} disabled={!canEdit || !!busy} /> Generate structures</label>
          </div>
          <button className="button button-primary" type="button" disabled={!canEdit || !!busy} onClick={() => void regenerateWorld()}><RotateCcw size={15} aria-hidden="true" /> {busy === "regenerate" ? "Preparing…" : "Generate again"}</button>
        </section>}
        {settingsLoading ? <p className="state-message">Reading world settings…</p> : settingsError ? <p className="files-error">Could not read world settings: {settingsError}</p> : settings && <>
          <div className="worlds-seed"><span>Current world seed</span><strong>{settings.actualSeed ?? "Unavailable"}</strong></div>
          <section className="worlds-gamerules" aria-label="Gamerules">
            <div><h4>Gamerules</h4><p>Rules and values come from the last saved world data. Send changes to a running active world.</p></div>
            <SearchField id="world-gamerule-search" label="Search gamerules" placeholder="Find a gamerule" value={ruleSearch} onChange={setRuleSearch} />
            {settings.gamerules.filter((rule) => rule.name.toLowerCase().includes(ruleSearch.trim().toLowerCase())).map((rule) => <div className="worlds-gamerule" key={rule.name}>
              <label htmlFor={`world-rule-${rule.name}`}>{rule.name}</label>
              {rule.value === "true" || rule.value === "false" ? <select id={`world-rule-${rule.name}`} className="worlds-rule-input" value={ruleValues[rule.name] ?? rule.value} onChange={(event) => setRuleValues((current) => ({ ...current, [rule.name]: event.currentTarget.value }))} disabled={!running || selectedWorld !== activeWorldName || !!busy}><option value="true">true</option><option value="false">false</option></select> : <input id={`world-rule-${rule.name}`} className="worlds-rule-input" value={ruleValues[rule.name] ?? rule.value} onChange={(event) => setRuleValues((current) => ({ ...current, [rule.name]: event.currentTarget.value }))} disabled={!running || selectedWorld !== activeWorldName || !!busy} />}
              <button className="button button-quiet" type="button" disabled={!running || selectedWorld !== activeWorldName || !!busy || ruleValues[rule.name] === rule.value || !validRuleValue(ruleValues[rule.name] ?? "")} onClick={() => void applyRule(rule.name)}>{busy === rule.name ? "Sending…" : "Apply"}</button>
            </div>)}
            {settings.gamerules.length === 0 && <p className="state-message">No gamerules were found in the saved world data.</p>}
            {settings.gamerules.length > 0 && !settings.gamerules.some((rule) => rule.name.toLowerCase().includes(ruleSearch.trim().toLowerCase())) && <p className="state-message">No matching gamerules.</p>}
          </section>
        </>}
      </div>}
      {selectedWorld && directory && (
        <div className="worlds-browser">
          <div className="worlds-browser-heading">
            <strong>{directory}</strong>
            {directory !== selectedWorld && <button className="button button-quiet" type="button" onClick={() => openDirectory(parent)}>Up one folder</button>}
          </div>
          <div className="files-browser">
            <div className="files-list">
              {isLoadingDirectory ? <p className="state-message">Loading files…</p> : entries.map((entry) => (
                <button className={`file-entry${selectedFile === entry.path ? " file-entry-selected" : ""}`} type="button" key={entry.path} onClick={() => void openFile(entry)}>
                  {entry.directory ? <Folder size={15} aria-hidden="true" /> : <File size={15} aria-hidden="true" />}
                  <span>{entry.name}</span>
                </button>
              ))}
              {!isLoadingDirectory && entries.length === 0 && <p className="state-message">This folder is empty.</p>}
            </div>
            <div className="worlds-preview">
              <strong>{selectedFile?.split("/").pop() ?? "File preview"}</strong>
              {preview !== null ? <pre>{preview}</pre> : <p>{previewError ?? "Select a text file to preview it. Binary files such as level.dat cannot be previewed."}</p>}
            </div>
          </div>
        </div>
      )}
      <section className="worlds-backups" aria-label="World backups">
        <div><h3>Backed up worlds</h3><p>Restore a chosen backup or the latest one for a world. Manual ZIPs stay until you delete them.</p></div>
        {backupsError && <p className="files-error" role="alert">Could not list backups: {backupsError}</p>}
        {backupsLoading ? <p className="state-message">Finding backups…</p> : backups.length === 0 ? <p className="state-message">No backups yet. Stop the server to create a manual backup, or enable automatic backups above.</p> : (
          <div className="worlds-backup-groups">{backupGroups.map((group) => <div className="worlds-backup-group" key={group.name}>
            <div className="worlds-backup-group-heading"><strong>{group.name}</strong><button className="button button-secondary" type="button" disabled={!canEdit || !!busy} onClick={() => void restoreBackup(group.entries[0])}>{busy === "restore" ? "Restoring…" : "Restore latest"}</button></div>
            <div className="worlds-backup-list">{group.entries.map((backup) => <div className="worlds-backup" key={backup.path}>
              <Archive size={16} aria-hidden="true" />
              <div><strong>{backup.automatic ? "Automatic ZIP" : backup.kind === "zip" ? "Manual ZIP" : "Preserved world folder"}</strong><small>{backup.modified ? new Date(backup.modified * 1000).toLocaleString() : "Date unavailable"}</small><code>{backup.path}</code></div>
              <button className="button button-quiet" type="button" disabled={!canEdit || !!busy} onClick={() => void restoreBackup(backup)}>Restore this</button>
              {backup.kind === "zip" && <button className="button button-quiet" type="button" disabled={!!busy} onClick={() => void deleteBackup(backup)}>Delete</button>}
            </div>)}</div>
          </div>)}</div>
        )}
      </section>
    </section>
  );
}
