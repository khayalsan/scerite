import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import { File, FilePlus2, Folder, Save, X } from "lucide-react";
import { serverFiles } from "../lib/serverFiles";
import type { ServerFileEntry } from "../lib/serverFiles";
import { SearchField } from "./SearchField";

interface LocalFilesPanelProps {
  profileId: string;
  serverName: string;
  canEdit: boolean;
  remote?: boolean;
  onClose: () => void;
}

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function formatSize(size: number) {
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

export function LocalFilesPanel({ profileId, serverName, canEdit, remote = false, onClose }: LocalFilesPanelProps) {
  const [directory, setDirectory] = useState("");
  const [entries, setEntries] = useState<ServerFileEntry[]>([]);
  const [search, setSearch] = useState("");
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [content, setContent] = useState("");
  const [originalContent, setOriginalContent] = useState("");
  const [fileName, setFileName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const hasUnsavedChanges = content !== originalContent;

  useEffect(() => {
    let active = true;
    setIsLoading(true);
    setError(null);
    void serverFiles.list(remote, profileId, directory)
      .then((files) => { if (active) setEntries(files); })
      .catch((reason: unknown) => { if (active) setError(errorText(reason)); })
      .finally(() => { if (active) setIsLoading(false); });
    return () => { active = false; };
  }, [directory, profileId, remote]);

  async function openFile(entry: ServerFileEntry) {
    if (entry.directory) {
      if (hasUnsavedChanges && !window.confirm("Discard unsaved changes?")) return;
      setDirectory(entry.path);
      setSelectedPath(null);
      setContent("");
      setOriginalContent("");
      setFileName("");
      return;
    }
    if (hasUnsavedChanges && !window.confirm("Discard unsaved changes?")) return;
    setError(null);
    try {
      const text = await serverFiles.read(remote, profileId, entry.path);
      setSelectedPath(entry.path);
      setContent(text);
      setOriginalContent(text);
      setFileName(entry.name);
    } catch (reason) {
      setError(errorText(reason));
    }
  }

  function createFile() {
    if (hasUnsavedChanges && !window.confirm("Discard unsaved changes?")) return;
    setSelectedPath(null);
    setContent("");
    setOriginalContent("");
    setFileName("");
    setError(null);
  }

  function closePanel() {
    if (!hasUnsavedChanges || window.confirm("Discard unsaved changes and close files?")) onClose();
  }

  function goUp() {
    if (hasUnsavedChanges && !window.confirm("Discard unsaved changes?")) return;
    setDirectory(parent);
    setSelectedPath(null);
    setContent("");
    setOriginalContent("");
    setFileName("");
  }

  async function saveFile(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const selectedSegments = selectedPath?.split("/");
    const name = selectedSegments ? selectedSegments[selectedSegments.length - 1] : fileName.trim();
    if (!name) return;
    const targetPath = selectedPath ?? (directory ? `${directory}/${name}` : name);
    const exists = entries.some((entry) => entry.path === targetPath);
    if (exists && !window.confirm(`Overwrite ${name}? This replaces the current server file.`)) return;

    setIsSaving(true);
    setError(null);
    try {
      await serverFiles.write(remote, profileId, targetPath, content, exists);
      setSelectedPath(targetPath);
      setFileName(name);
      setOriginalContent(content);
      const updated = await serverFiles.list(remote, profileId, directory);
      setEntries(updated);
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setIsSaving(false);
    }
  }

  const parent = directory.includes("/") ? directory.slice(0, directory.lastIndexOf("/")) : "";
  const selectedName = selectedPath?.split("/").slice(-1)[0];
  const visibleEntries = entries.filter((entry) => entry.name.toLowerCase().includes(search.trim().toLowerCase()));

  return (
    <section className="files-panel" aria-label={`${serverName} files`}>
      <header className="files-header">
        <div>
          <strong>Server files</strong>
          <span>{directory || "/"}</span>
        </div>
        <button className="icon-button" aria-label="Close files" onClick={closePanel}><X size={17} /></button>
      </header>
      {error && <p className="files-error" role="alert">{error}</p>}
      <div className="files-browser">
        <div className="files-list">
          <div className="files-list-actions">
            {directory && <button className="button button-quiet" onClick={goUp}>Up one folder</button>}
            <button className="button button-quiet" onClick={createFile} disabled={!canEdit}><FilePlus2 size={14} /> New file</button>
          </div>
          <SearchField id="files-search" label="Search this folder" placeholder="File or folder name" value={search} onChange={setSearch} />
          {isLoading ? <p className="state-message">Loading files…</p> : visibleEntries.map((entry) => (
            <button className={`file-entry${selectedPath === entry.path ? " file-entry-selected" : ""}`} key={entry.path} onClick={() => void openFile(entry)}>
              {entry.directory ? <Folder size={15} /> : <File size={15} />}
              <span>{entry.name}</span>
              {!entry.directory && <small>{formatSize(entry.size)}</small>}
            </button>
          ))}
          {!isLoading && entries.length === 0 && <p className="state-message">This folder is empty.</p>}
          {!isLoading && entries.length > 0 && visibleEntries.length === 0 && <p className="state-message">No matching files or folders.</p>}
        </div>
        <form className="file-editor" onSubmit={(event) => void saveFile(event)}>
          {selectedPath || fileName !== "" ? (
            <div className="file-editor-heading">
              <strong>{selectedName ?? "New file"}</strong>
              <button className="button button-primary" type="submit" disabled={!canEdit || isSaving}>{isSaving ? "Saving…" : <><Save size={14} /> Save</>}</button>
            </div>
          ) : (
            <div className="file-editor-heading">
              <input aria-label="New file name" value={fileName} onChange={(event) => setFileName(event.currentTarget.value)} placeholder="config.txt" disabled={!canEdit} />
              <button className="button button-primary" type="submit" disabled={!canEdit || isSaving || !fileName.trim()}>{isSaving ? "Saving…" : <><Save size={14} /> Save</>}</button>
            </div>
          )}
          <textarea aria-label="File contents" value={content} onChange={(event) => setContent(event.currentTarget.value)} placeholder="Select a file or create a new text file" spellCheck={false} disabled={!canEdit} />
          <small>{canEdit ? "Text files up to 1 MB. Overwriting an existing file asks for confirmation." : "Stop the server before editing files."}</small>
        </form>
      </div>
    </section>
  );
}
