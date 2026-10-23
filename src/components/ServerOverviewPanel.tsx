import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ServerEnvironment, ServerProfile } from "../lib/serverProfiles";
import { serverFiles } from "../lib/serverFiles";
import { serverSessions } from "../lib/serverSessions";
import { ServerIconControl } from "./ServerIconControl";

type Usage = { playersOnline: number | null; maxPlayers: number | null; cpuPercent: number | null; memoryBytes: number | null };

function memoryLabel(bytes: number | null | undefined): string {
  if (bytes == null) return "Unavailable";
  return `${(bytes / 1024 / 1024).toFixed(0)} MB`;
}

function tpsFromLines(lines: string[]): number | null {
  // tmux may wrap a long formatted response across multiple pane lines.
  const plain = lines.join(" ").replace(/\x1b\[[0-9;]*[a-z]/gi, "").replace(/§[0-9a-fk-or]/gi, "");
  const matches = [...plain.matchAll(/\b(?:TPS\s+from\s+last[^:]*|(?:current\s+)?TPS)\s*[:=]\s*\*?([0-9]+(?:\.[0-9]+)?)/gi)];
  const value = matches[matches.length - 1]?.[1];
  return value === undefined ? null : Math.min(20, Number(value));
}

function newRemoteLines(before: string[], after: string[]): string[] {
  before = [...before];
  after = [...after];
  while (before.length && before[before.length - 1].trim() === "") before.pop();
  while (after.length && after[after.length - 1].trim() === "") after.pop();
  if (before.length === after.length && before.every((line, index) => line === after[index])) return [];
  for (let overlap = Math.min(before.length, after.length); overlap > 0; overlap--) {
    if (before.slice(-overlap).every((line, index) => line === after[index])) return after.slice(overlap);
  }
  // A tmux pane can redraw its last line instead of appending to it.
  for (let index = before.length - 1; index >= Math.max(0, before.length - 20); index--) {
    const anchor = after.lastIndexOf(before[index]);
    if (anchor >= 0) return after.slice(anchor + 1);
  }
  return before.length === 0 ? after : [];
}

async function readTps(profileId: string, remote: boolean): Promise<number | null> {
  let previousRemote = remote ? (await serverSessions.remoteConsole(profileId)).lines : [];
  let localCursor = remote ? 0 : (await serverSessions.console(profileId, 0)).nextSequence;
  const freshLines: string[] = [];
  await (remote ? serverSessions.sendRemoteInput(profileId, "tps") : serverSessions.sendInput(profileId, "tps"));
  for (const delay of [500, 800, 1200, 1500, 2000]) {
    await new Promise((resolve) => window.setTimeout(resolve, delay));
    if (remote) {
      const next = (await serverSessions.remoteConsole(profileId)).lines;
      freshLines.push(...newRemoteLines(previousRemote, next));
      previousRemote = next;
    } else {
      const next = await serverSessions.console(profileId, localCursor);
      freshLines.push(...next.lines.map((line) => line.text));
      localCursor = next.nextSequence;
    }
    const tps = tpsFromLines(freshLines);
    if (tps !== null) return tps;
  }
  return null;
}

export function ServerOverviewPanel({ profile, environment, state }: { profile: ServerProfile; environment?: ServerEnvironment; state: "online" | "offline" | "checking" }) {
  const remote = profile.target.kind === "remote";
  const installedSoftware = profile.installedSoftware?.toLowerCase() ?? "";
  const supportsTps = ["paper", "purpur", "spigot", "bukkit", "folia"].includes(installedSoftware) || installedSoftware.startsWith("arclight-");
  const [port, setPort] = useState<number | null>(null);
  const [usage, setUsage] = useState<Usage | null>(null);
  const [detectedVersion, setDetectedVersion] = useState<string | null>(null);
  const [tps, setTps] = useState<number | null>(null);
  const [tpsLoading, setTpsLoading] = useState(false);

  useEffect(() => {
    let active = true;
    setPort(null);
    setDetectedVersion(null);
    void serverFiles.read(remote, profile.id, "server.properties")
      .then((properties) => {
        const match = /^server-port\s*=\s*(\d+)\s*$/m.exec(properties);
        const parsed = Number(match?.[1]);
        if (active) setPort(Number.isInteger(parsed) && parsed > 0 && parsed <= 65535 ? parsed : 25565);
      })
      .catch(() => { if (active) setPort(25565); });
    if (!profile.installedVersion) {
      void invoke<string>(remote ? "read_remote_server_log_start" : "read_local_server_log_start", { profileId: profile.id })
        .then((log) => {
          const version = log.match(/(?:Starting minecraft server version|Loading Minecraft)\s+([0-9][a-zA-Z0-9._-]*)/i)?.[1];
          if (active && version) setDetectedVersion(version);
        }).catch(() => {});
    }
    return () => { active = false; };
  }, [profile.id, profile.target.directory, profile.installedVersion, remote, state]);

  useEffect(() => {
    if (state !== "online" || port === null) { setUsage(null); return; }
    let active = true;
    let timer: number | undefined;
    const refresh = async () => {
      try {
        const next = await invoke<Usage>("server_usage", { profileId: profile.id, port });
        if (active) setUsage(next);
      } catch {
        if (active) setUsage(null);
      } finally {
        if (active) timer = window.setTimeout(() => void refresh(), 5000);
      }
    };
    void refresh();
    return () => { active = false; if (timer !== undefined) window.clearTimeout(timer); };
  }, [profile.id, port, state]);

  useEffect(() => {
    setTps(null);
    if (state !== "online") { setTpsLoading(false); return; }
    let active = true;
    let timer: number | undefined;
    const refresh = async () => {
      if (active) setTpsLoading(true);
      let received = false;
      try {
        const next = await readTps(profile.id, remote);
        received = next !== null;
        if (active) setTps(next);
      } catch {
        if (active) setTps(null);
      } finally {
        if (active) {
          setTpsLoading(false);
          if (supportsTps || received) timer = window.setTimeout(() => void refresh(), 30000);
        }
      }
    };
    void refresh();
    return () => { active = false; if (timer !== undefined) window.clearTimeout(timer); };
  }, [profile.id, remote, state, supportsTps]);

  const host = remote ? environment?.host ?? "Unknown host" : "localhost";
  const address = `${host.includes(":") && !host.startsWith("[") ? `[${host}]` : host}:${port ?? "…"}`;
  const software = profile.installedSoftware ? profile.installedSoftware[0].toUpperCase() + profile.installedSoftware.slice(1) : "Unknown";
  const players = state === "offline" ? "0" : usage?.playersOnline == null ? "Unavailable" : `${usage.playersOnline} / ${usage.maxPlayers ?? "?"}`;

  return <section className="server-overview" aria-label="Server overview">
    <header><h2>Server</h2><p>Current status and setup</p></header>
    <dl className="server-overview-details">
      <div><dt>Status</dt><dd className={`server-overview-status server-overview-status-${state}`}>{state === "checking" ? "Checking…" : state === "online" ? "Online" : "Offline"}</dd></div>
      <div><dt>Players</dt><dd>{players}</dd></div>
      <div><dt>Address</dt><dd>{address}</dd></div>
      <div><dt>Software</dt><dd>{software}</dd></div>
      <div><dt>Version</dt><dd>{profile.installedVersion ?? detectedVersion ?? "Unknown"}</dd></div>
      <div><dt>CPU</dt><dd>{state === "online" && usage?.cpuPercent != null ? `${usage.cpuPercent.toFixed(1)}%` : state === "offline" ? "0%" : "Unavailable"}</dd></div>
      <div><dt>RAM</dt><dd>{state === "offline" ? "0 MB" : memoryLabel(usage?.memoryBytes)}</dd></div>
      <div><dt>TPS (1m)</dt><dd title={tps !== null ? "Average ticks per second over the last minute; refreshed every 30 seconds." : "Scerite reads TPS from this server's console command when available."}>{state === "online" ? tps == null ? tpsLoading ? "Checking…" : supportsTps ? "Unavailable" : "No TPS response" : tps.toFixed(1) : "—"}</dd></div>
    </dl>
    <ServerIconControl key={profile.id} profileId={profile.id} remote={remote} running={state !== "offline"} />
  </section>;
}
