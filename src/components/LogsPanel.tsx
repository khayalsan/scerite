import { useEffect, useRef, useState } from "react";
import { serverFiles } from "../lib/serverFiles";
import { serverLogs } from "../lib/serverLogs";
import type { ServerLogSnapshot } from "../lib/serverLogs";
import { displayCache } from "../lib/displayCache";
import type { CachedLogs } from "../lib/displayCache";
import { SearchField } from "./SearchField";
import { ConsoleLogLine, isChatConsoleLine, plainConsoleLine } from "./ConsoleLogLine";

interface LogsPanelProps {
  profileId: string;
  remote: boolean;
  running: boolean;
}

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function LogsPanel({ profileId, remote, running }: LogsPanelProps) {
  const [names, setNames] = useState<string[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [snapshot, setSnapshot] = useState<ServerLogSnapshot | null>(null);
  const [search, setSearch] = useState("");
  const [chatOnly, setChatOnly] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [revision, setRevision] = useState(0);
  const [showingCached, setShowingCached] = useState(false);
  const cachedLogsRef = useRef<CachedLogs>({ names: [], latest: null });

  function saveCache(next: Partial<CachedLogs>) {
    cachedLogsRef.current = { ...cachedLogsRef.current, ...next };
    void displayCache.writeLogs(profileId, cachedLogsRef.current).catch(() => {});
  }

  useEffect(() => {
    let active = true;
    setLoading(true);
    void (async () => {
      if (remote) {
        try {
          const cached = await displayCache.readLogs(profileId);
          if (active && cached && Array.isArray(cached.names) && cached.names.length <= 1000
            && cached.names.every((name) => typeof name === "string" && /^[A-Za-z0-9._-]{1,128}\.log(?:\.gz)?$/.test(name) && !name.includes(".."))) {
            cachedLogsRef.current = { names: cached.names, latest: cached.latest && typeof cached.latest.text === "string"
              && typeof cached.latest.truncated === "boolean" ? cached.latest : null };
            setNames(cached.names);
            setSelected((current) => current && cached.names.includes(current) ? current : cached.names[0] ?? null);
            if (cached.names.length) setLoading(false);
          }
        } catch { /* Remote logs remain available when the local cache cannot be read. */ }
      }
      return serverFiles.list(remote, profileId, "logs");
    })().then((entries) => {
      if (!active) return;
      const found = entries.filter((entry) => !entry.directory && /^[A-Za-z0-9._-]{1,128}\.log(?:\.gz)?$/.test(entry.name) && !entry.name.includes(".."))
        .map((entry) => entry.name)
        .sort((left, right) => left === "latest.log" ? -1 : right === "latest.log" ? 1 : right.localeCompare(left));
      setNames(found);
      setSelected((current) => current && found.includes(current) ? current : found[0] ?? null);
      setError(null);
      if (remote) saveCache({ names: found });
    }).catch((reason: unknown) => {
      if (active) setError(`Could not list server logs: ${errorText(reason)}`);
    }).finally(() => {
      if (active) setLoading(false);
    });
    return () => { active = false; };
  }, [profileId, remote, revision]);

  useEffect(() => {
    if (!selected) { setSnapshot(null); return; }
    let active = true;
    const read = async () => {
      try {
        const result = await serverLogs.read(remote, profileId, selected);
        if (active) { setSnapshot(result); setShowingCached(false); setError(null); }
        if (remote && selected === "latest.log") saveCache({ latest: result });
      } catch (reason) {
        if (active) setError(`Could not open ${selected}: ${errorText(reason)}`);
      }
    };
    const cached = remote && selected === "latest.log" ? cachedLogsRef.current.latest : null;
    setSnapshot(cached);
    setShowingCached(!!cached);
    void read();
    const timer = selected === "latest.log" && running ? window.setInterval(() => void read(), 5000) : undefined;
    return () => { active = false; if (timer !== undefined) window.clearInterval(timer); };
  }, [profileId, remote, running, selected, revision]);

  const normalizedSearch = search.trim().toLowerCase();
  const visibleLines = snapshot ? snapshot.text.split(/\r?\n/)
    .filter((line) => !chatOnly || isChatConsoleLine(line))
    .filter((line) => !normalizedSearch || plainConsoleLine(line).toLowerCase().includes(normalizedSearch)) : [];

  return <div className="logs-panel">
    <header className="logs-header">
      <div><h2>Server logs</h2><p>Read the latest output or open an older log.</p></div>
      <div className="logs-header-actions">
        <label className="logs-chat-toggle">Chat only <span className="property-switch"><input type="checkbox" role="switch" checked={chatOnly} onChange={(event) => setChatOnly(event.currentTarget.checked)} /><span aria-hidden="true" /></span></label>
        <button className="button button-quiet" type="button" onClick={() => setRevision((value) => value + 1)}>Refresh</button>
      </div>
    </header>
    <SearchField id="logs-search" label="Search selected log" placeholder="Find matching log lines" value={search} onChange={setSearch} />
    {error && <p className="notice notice-error" role="alert">{error}</p>}
    {loading ? <p className="state-message">Loading logs…</p> : names.length === 0 ? <p className="state-message">No server logs are available yet.</p> :
      <div className="logs-layout">
        <nav className="logs-list" aria-label="Available logs">
          {names.map((name) => <button key={name} type="button" className={selected === name ? "logs-item logs-item-active" : "logs-item"} onClick={() => { setError(null); setSelected(name); }} aria-current={selected === name ? "true" : undefined}>{name}</button>)}
        </nav>
        <section className="logs-reader" aria-label={selected ? `${selected} contents` : "Log contents"}>
          <div className="logs-reader-heading"><strong>{selected}</strong><span>{showingCached ? "Cached · updating…" : normalizedSearch && snapshot ? `${visibleLines.length} matching lines${snapshot.truncated ? " · last 256 KB" : ""}` : snapshot?.truncated ? "Showing the last 256 KB" : ""}</span></div>
          <pre>{snapshot ? visibleLines.length ? visibleLines.map((line, index) => <ConsoleLogLine key={index} line={line} trailingNewline={index < visibleLines.length - 1} />) : chatOnly ? "No chat messages found in this log." : normalizedSearch ? "No matching lines in this log." : "" : "Loading log…"}</pre>
        </section>
      </div>}
  </div>;
}
