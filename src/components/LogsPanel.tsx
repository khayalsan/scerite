import { useEffect, useState } from "react";
import { serverFiles } from "../lib/serverFiles";
import { serverLogs } from "../lib/serverLogs";
import type { ServerLogSnapshot } from "../lib/serverLogs";
import { SearchField } from "./SearchField";

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
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [revision, setRevision] = useState(0);

  useEffect(() => {
    let active = true;
    setLoading(true);
    void serverFiles.list(remote, profileId, "logs").then((entries) => {
      if (!active) return;
      const found = entries.filter((entry) => !entry.directory && /^[A-Za-z0-9._-]{1,128}\.log(?:\.gz)?$/.test(entry.name) && !entry.name.includes(".."))
        .map((entry) => entry.name)
        .sort((left, right) => left === "latest.log" ? -1 : right === "latest.log" ? 1 : right.localeCompare(left));
      setNames(found);
      setSelected((current) => current && found.includes(current) ? current : found[0] ?? null);
      setError(null);
    }).catch((reason: unknown) => {
      if (active) { setNames([]); setError(`Could not list server logs: ${errorText(reason)}`); }
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
        if (active) { setSnapshot(result); setError(null); }
      } catch (reason) {
        if (active) setError(`Could not open ${selected}: ${errorText(reason)}`);
      }
    };
    setSnapshot(null);
    void read();
    const timer = selected === "latest.log" && running ? window.setInterval(() => void read(), 5000) : undefined;
    return () => { active = false; if (timer !== undefined) window.clearInterval(timer); };
  }, [profileId, remote, running, selected, revision]);

  const normalizedSearch = search.trim().toLowerCase();
  const matchingLines = normalizedSearch && snapshot ? snapshot.text.split(/\r?\n/).filter((line) => line.toLowerCase().includes(normalizedSearch)) : [];
  const visibleText = snapshot ? normalizedSearch ? matchingLines.join("\n") : snapshot.text : "Loading log…";

  return <div className="logs-panel">
    <header className="logs-header">
      <div><h2>Server logs</h2><p>Read the latest output or open an older log.</p></div>
      <button className="button button-quiet" type="button" onClick={() => setRevision((value) => value + 1)}>Refresh</button>
    </header>
    <SearchField id="logs-search" label="Search selected log" placeholder="Find matching log lines" value={search} onChange={setSearch} />
    {error && <p className="notice notice-error" role="alert">{error}</p>}
    {loading ? <p className="state-message">Loading logs…</p> : names.length === 0 ? <p className="state-message">No server logs are available yet.</p> :
      <div className="logs-layout">
        <nav className="logs-list" aria-label="Available logs">
          {names.map((name) => <button key={name} type="button" className={selected === name ? "logs-item logs-item-active" : "logs-item"} onClick={() => { setError(null); setSelected(name); }} aria-current={selected === name ? "true" : undefined}>{name}</button>)}
        </nav>
        <section className="logs-reader" aria-label={selected ? `${selected} contents` : "Log contents"}>
          <div className="logs-reader-heading"><strong>{selected}</strong><span>{normalizedSearch && snapshot ? `${matchingLines.length} matching lines${snapshot.truncated ? " · last 256 KB" : ""}` : snapshot?.truncated ? "Showing the last 256 KB" : ""}</span></div>
          <pre>{visibleText || (normalizedSearch ? "No matching lines in this log." : "")}</pre>
        </section>
      </div>}
  </div>;
}
