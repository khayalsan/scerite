import { useEffect, useRef, useState } from "react";
import { File, Folder, Globe2, RefreshCw } from "lucide-react";
import { serverFiles } from "../lib/serverFiles";
import type { ServerFileEntry } from "../lib/serverFiles";

interface WorldsPanelProps {
  profileId: string;
  remote: boolean;
}

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function worldName(properties: string) {
  const setting = properties.split(/\r?\n/).find((line) => /^\s*level-name\s*=/.test(line));
  return setting?.slice(setting.indexOf("=") + 1).trim() || "world";
}

export function WorldsPanel({ profileId, remote }: WorldsPanelProps) {
  const [worlds, setWorlds] = useState<ServerFileEntry[]>([]);
  const [activeWorldName, setActiveWorldName] = useState("world");
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
  const previewRequest = useRef(0);

  useEffect(() => {
    let active = true;
    setIsLoading(true);
    setError(null);
    void (async () => {
      const root = await serverFiles.list(remote, profileId, "");
      const properties = await serverFiles.read(remote, profileId, "server.properties").catch(() => "");
      const candidates = root.filter((entry) => entry.directory);
      const listings = await Promise.allSettled(candidates.map((entry) => serverFiles.list(remote, profileId, entry.path)));
      if (!active) return;
      setActiveWorldName(worldName(properties));
      setWorlds(candidates.filter((_, index) => listings[index].status === "fulfilled" && listings[index].value.some((file) => file.name === "level.dat" && !file.directory)));
      if (listings.some((result) => result.status === "rejected")) {
        setError("Some folders could not be inspected. The world list may be incomplete.");
      }
    })().catch((reason: unknown) => { if (active) setError(errorText(reason)); })
      .finally(() => { if (active) setIsLoading(false); });
    return () => { active = false; };
  }, [profileId, remote, revision]);

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

  return (
    <section className="worlds-panel" aria-label="World browser">
      <header className="worlds-header">
        <div>
          <h2>Worlds</h2>
          <p>World folders in this server. Browse files without changing them.</p>
        </div>
        <button className="button button-quiet" type="button" onClick={() => setRevision((value) => value + 1)} disabled={isLoading}>
          <RefreshCw size={14} aria-hidden="true" /> Refresh
        </button>
      </header>
      {error && <p className="files-error" role="alert">{error}</p>}
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
    </section>
  );
}
