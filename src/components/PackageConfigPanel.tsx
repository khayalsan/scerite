import { useEffect, useMemo, useState } from "react";
import { ArrowLeft, Folder, Save } from "lucide-react";
import { serverFiles } from "../lib/serverFiles";
import type { ServerFileEntry } from "../lib/serverFiles";
import { applyConfigEdits, configFields, configFormat } from "../lib/packageConfig";
import type { ConfigValue } from "../lib/packageConfig";
import { SearchField } from "./SearchField";

const textExtensions = /\.(json|ya?ml|toml|properties|cfg|conf|txt|json5|hocon)$/i;
const comparable = (value: string) => value.toLowerCase().replace(/[^a-z0-9]/g, "");
const errorText = (error: unknown) => error instanceof Error ? error.message : String(error);

export function PackageConfigPanel({ profileId, remote, canEdit, kind, title, filename, onClose }: {
  profileId: string; remote: boolean; canEdit: boolean; kind: "mod" | "plugin"; title: string; filename: string; onClose: () => void;
}) {
  const root = kind === "mod" ? "config" : "plugins";
  const [folder, setFolder] = useState(root);
  const [entries, setEntries] = useState<ServerFileEntry[]>([]);
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<ServerFileEntry | null>(null);
  const [original, setOriginal] = useState("");
  const [draft, setDraft] = useState("");
  const [edits, setEdits] = useState<Record<string, ConfigValue>>({});
  const [mode, setMode] = useState<"form" | "raw">("form");
  const [loadingFolder, setLoadingFolder] = useState(true);
  const [loadingFile, setLoadingFile] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const dirty = draft !== original || Object.keys(edits).length > 0;
  const fields = useMemo(() => {
    if (!selected) return { items: [], error: null };
    try { return { items: configFields(selected.path, draft), error: null }; }
    catch (cause) { return { items: [], error: errorText(cause) }; }
  }, [selected, draft]);

  useEffect(() => {
    let active = true;
    setLoadingFolder(true);
    void serverFiles.list(remote, profileId, root).then((items) => {
      if (!active) return;
      if (kind === "plugin") {
        const name = comparable(title);
        const jarName = comparable(filename.replace(/\.jar$/i, "").replace(/[-_][0-9].*$/, ""));
        const match = items.find((item) => item.directory && [name, jarName].some((candidate) => candidate && comparable(item.name) === candidate));
        if (match) { setFolder(match.path); return; }
      }
      setEntries(items);
      setLoadingFolder(false);
    }).catch((cause) => {
      if (active) { setError(`Could not browse ${root}/: ${errorText(cause)}`); setLoadingFolder(false); }
    });
    return () => { active = false; };
  }, [profileId, remote, root, kind, title, filename]);

  useEffect(() => {
    let active = true;
    setLoadingFolder(true);
    void serverFiles.list(remote, profileId, folder).then((items) => {
      if (active) { setEntries(items); setLoadingFolder(false); }
    }).catch((cause) => {
      if (active) { setEntries([]); setError(`Could not browse ${folder}/: ${errorText(cause)}`); setLoadingFolder(false); }
    });
    return () => { active = false; };
  }, [profileId, remote, folder]);

  function discardChanges() { return !dirty || window.confirm("Discard unsaved configuration changes?"); }

  async function openEntry(entry: ServerFileEntry) {
    if (!discardChanges()) return;
    setError(null);
    setMessage(null);
    if (entry.directory) {
      setFolder(entry.path);
      setSelected(null);
      setOriginal("");
      setDraft("");
      setEdits({});
      setSearch("");
      return;
    }
    setLoadingFile(true);
    try {
      const content = await serverFiles.read(remote, profileId, entry.path);
      setSelected(entry);
      setOriginal(content);
      setDraft(content);
      setEdits({});
      try { setMode(configFields(entry.path, content).length ? "form" : "raw"); }
      catch { setMode("raw"); }
    } catch (cause) { setError(`Could not open ${entry.name}: ${errorText(cause)}`); }
    finally { setLoadingFile(false); }
  }

  function goUp() {
    if (!discardChanges()) return;
    setFolder(folder.slice(0, folder.lastIndexOf("/")) || root);
    setSelected(null);
    setOriginal("");
    setDraft("");
    setEdits({});
    setSearch("");
  }

  function showRaw() {
    if (!selected) return;
    try {
      if (Object.keys(edits).length) setDraft(applyConfigEdits(selected.path, draft, edits));
      setEdits({});
      setMode("raw");
      setError(null);
    } catch (cause) { setError(errorText(cause)); }
  }

  function showForm() {
    if (!selected) return;
    try {
      if (!configFields(selected.path, draft).length) throw new Error("This file has no editable fields. Use Raw view.");
      setEdits({});
      setMode("form");
      setError(null);
    } catch (cause) { setError(errorText(cause)); }
  }

  async function save() {
    if (!selected || !canEdit || saving) return;
    let next: string;
    try { next = mode === "form" ? applyConfigEdits(selected.path, draft, edits) : draft; }
    catch (cause) { setError(errorText(cause)); return; }
    if (next === original) return;
    const warning = mode === "form" && configFormat(selected.path) === "toml"
      ? "Form saving rewrites TOML formatting and comments. Use Raw view to preserve them exactly. Save anyway?"
      : `Overwrite ${selected.name}? This replaces the current server file.`;
    if (!window.confirm(warning)) return;
    setSaving(true);
    setError(null);
    setMessage(null);
    try {
      const latest = await serverFiles.read(remote, profileId, selected.path);
      if (latest !== original) throw new Error("This config changed since you opened it. Reopen the file before saving.");
      await serverFiles.write(remote, profileId, selected.path, next, true);
      setOriginal(next);
      setDraft(next);
      setEdits({});
      setMessage(`Saved ${selected.name}. Restart the server if the package requires it.`);
    } catch (cause) { setError(`Could not save ${selected.name}: ${errorText(cause)}`); }
    finally { setSaving(false); }
  }

  const name = comparable(title);
  const jarName = comparable(filename.replace(/\.jar$/i, "").replace(/[-_][0-9].*$/, ""));
  const visibleEntries = entries.filter((entry) => entry.directory || textExtensions.test(entry.name))
    .filter((entry) => entry.name.toLowerCase().includes(search.trim().toLowerCase()))
    .sort((left, right) => Number(right.directory) - Number(left.directory)
      || Number([name, jarName].some((candidate) => candidate && comparable(right.name).includes(candidate)))
        - Number([name, jarName].some((candidate) => candidate && comparable(left.name).includes(candidate)))
      || left.name.localeCompare(right.name));

  return <div className="package-config-panel">
    <header className="package-config-header"><button type="button" className="button button-quiet" onClick={() => { if (discardChanges()) onClose(); }}><ArrowLeft size={15} aria-hidden="true" /> Back to {kind === "mod" ? "mods" : "plugins"}</button><div><h3>Configure {title}</h3><p>Choose a file in {root}/. Suggested names appear first; browse folders if this package uses a different name.</p></div></header>
    {error && <p className="files-error" role="alert">{error}</p>}
    {message && <p className="install-success" role="status">{message}</p>}
    <div className="package-config-layout">
      <aside className="package-config-files" aria-label="Configuration files">
        <div className="package-config-folder"><strong>{folder}/</strong>{folder !== root && <button type="button" className="button button-quiet" onClick={goUp}>Up one folder</button>}</div>
        <SearchField id="package-config-search" label="Find a config file" placeholder="File or folder name" value={search} onChange={setSearch} />
        {loadingFolder ? <p className="state-message">Loading files…</p> : visibleEntries.length ? visibleEntries.map((entry) => <button key={entry.path} type="button" className={`package-config-file${selected?.path === entry.path ? " active" : ""}`} onClick={() => void openEntry(entry)}><span>{entry.directory && <Folder size={14} aria-hidden="true" />}{entry.name}</span>{!entry.directory && <small>{configFormat(entry.path) === "raw" ? "Raw" : configFormat(entry.path).toUpperCase()}</small>}</button>) : <p className="state-message">No configuration files found here.</p>}
      </aside>
      <section className="package-config-editor" aria-label="Configuration editor">
        {!selected ? <p className="state-message">Select a configuration file to view its settings.</p> : loadingFile ? <p className="state-message">Opening file…</p> : <>
          <div className="package-config-editor-header"><div><strong>{selected.name}</strong><small>{selected.path}</small></div><div className="package-config-actions"><button type="button" className="button button-quiet" aria-pressed={mode === "form"} onClick={showForm} disabled={!fields.items.length}>Form</button><button type="button" className="button button-quiet" aria-pressed={mode === "raw"} onClick={showRaw}>Raw</button><button type="button" className="button button-primary" onClick={() => void save()} disabled={!canEdit || saving || !dirty}><Save size={14} aria-hidden="true" /> {saving ? "Saving…" : "Save"}</button></div></div>
          {!canEdit && <p className="players-caption">Stop the server before changing configuration files.</p>}
          {mode === "form" && fields.error && <p className="files-error" role="alert">Could not parse this file: {fields.error}. Use Raw view.</p>}
          {mode === "form" ? <div className="package-config-fields">{fields.items.map((field) => {
            const key = JSON.stringify(field.path);
            const value = edits[key] ?? field.value;
            return <label className="package-config-field" key={key}><span>{field.label || "Value"}</span>{typeof field.value === "boolean" ? <input type="checkbox" checked={value === true} onChange={(event) => setEdits({ ...edits, [key]: event.target.checked })} disabled={!canEdit} /> : <input type={typeof field.value === "number" ? "number" : "text"} step={typeof field.value === "number" ? "any" : undefined} value={String(value)} onChange={(event) => setEdits({ ...edits, [key]: event.target.value })} disabled={!canEdit} />}</label>;
          })}</div> : <textarea className="package-config-raw" aria-label="Raw configuration" value={draft} onChange={(event) => setDraft(event.target.value)} spellCheck={false} disabled={!canEdit} />}
          <small className="package-config-note">Form editing preserves other settings. TOML form saves may change formatting and comments; Raw view keeps the text you write.</small>
        </>}
      </section>
    </div>
  </div>;
}
