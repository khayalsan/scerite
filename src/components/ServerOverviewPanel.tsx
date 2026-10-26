import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ServerEnvironment, ServerProfile } from "../lib/serverProfiles";
import { serverFiles } from "../lib/serverFiles";
import { ServerIconControl } from "./ServerIconControl";
import { velocityOnDemand } from "../lib/velocityOnDemand";
import type { VelocityOnDemandStatus } from "../lib/velocityOnDemand";

type Usage = { playersOnline: number | null; maxPlayers: number | null; cpuPercent: number | null; memoryBytes: number | null };

function memoryLabel(bytes: number | null | undefined): string {
  if (bytes == null) return "Unavailable";
  return `${(bytes / 1024 / 1024).toFixed(0)} MB`;
}

export function ServerOverviewPanel({ profile, environment, state }: { profile: ServerProfile; environment?: ServerEnvironment; state: "online" | "offline" | "checking" }) {
  const remote = profile.target.kind === "remote";
  const [port, setPort] = useState<number | null>(null);
  const [usage, setUsage] = useState<Usage | null>(null);
  const [detectedVersion, setDetectedVersion] = useState<string | null>(null);
  const [velocity, setVelocity] = useState<VelocityOnDemandStatus | null>(null);
  const [publicPortDraft, setPublicPortDraft] = useState("");
  const [reachability, setReachability] = useState<{ port: number; reachable: boolean } | null>(null);
  const [reachabilityError, setReachabilityError] = useState<string | null>(null);
  const [checkingReachability, setCheckingReachability] = useState(false);
  const reachabilityRequest = useRef(0);

  useEffect(() => {
    setVelocity(null);
    if (!remote || !["paper", "purpur"].includes(profile.installedSoftware ?? "")) return;
    let active = true;
    void velocityOnDemand.status(profile.id).then((value) => { if (active) setVelocity(value); }).catch(() => {});
    return () => { active = false; };
  }, [profile.id, profile.installedSoftware, remote]);

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

  const host = remote ? environment?.host ?? "Unknown host" : "localhost";
  const publicPort = velocity?.enabled ? velocity.publicPort : port;
  const address = `${host.includes(":") && !host.startsWith("[") ? `[${host}]` : host}:${publicPort ?? "…"}`;
  const software = profile.installedSoftware ? profile.installedSoftware[0].toUpperCase() + profile.installedSoftware.slice(1) : "Unknown";
  const players = state === "offline" ? "0" : usage?.playersOnline == null ? "Unavailable" : `${usage.playersOnline} / ${usage.maxPlayers ?? "?"}`;

  useEffect(() => {
    reachabilityRequest.current += 1;
    setPublicPortDraft(publicPort === null ? "" : String(publicPort));
    setReachability(null);
    setReachabilityError(null);
    setCheckingReachability(false);
  }, [profile.id, publicPort]);

  async function checkReachability() {
    const checkPort = Number(publicPortDraft);
    if (!Number.isInteger(checkPort) || checkPort < 1 || checkPort > 65535) {
      setReachabilityError("Enter a port between 1 and 65535.");
      return;
    }
    const request = ++reachabilityRequest.current;
    setCheckingReachability(true);
    setReachability(null);
    setReachabilityError(null);
    try {
      const reachable = await invoke<boolean>(remote ? "check_remote_public_port" : "check_local_public_port", remote ? { profileId: profile.id, port: checkPort } : { port: checkPort });
      if (request === reachabilityRequest.current) setReachability({ port: checkPort, reachable });
    } catch (reason) {
      if (request === reachabilityRequest.current) setReachabilityError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      if (request === reachabilityRequest.current) setCheckingReachability(false);
    }
  }

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
    </dl>
    <section className="server-reachability" aria-labelledby="server-reachability-title">
      <h3 id="server-reachability-title">Public port check</h3>
      <p>Check whether this {remote ? "remote host's" : "computer's"} public IP accepts TCP connections on a port.</p>
      <div className="server-reachability-actions">
        <label htmlFor="server-public-port">Public port</label>
        <input id="server-public-port" type="number" min="1" max="65535" value={publicPortDraft} onChange={(event) => { reachabilityRequest.current += 1; setPublicPortDraft(event.currentTarget.value); setReachability(null); setReachabilityError(null); setCheckingReachability(false); }} />
        <button className="button button-secondary" type="button" disabled={checkingReachability} onClick={() => void checkReachability()}>{checkingReachability ? "Checking…" : "Check port"}</button>
      </div>
      {reachability && <p role="status" className={reachability.reachable ? "server-reachability-success" : "server-reachability-failure"}>Port {reachability.port} is {reachability.reachable ? "reachable" : "not reachable"} from the public checker.</p>}
      {reachabilityError && <p className="form-error" role="alert">{reachabilityError}</p>}
      <small>The checker tests the public IP this {remote ? "remote host" : "computer"} uses for outgoing requests. This may differ from the address players use. A reachable TCP port does not confirm Minecraft is accepting players.</small>
    </section>
    <ServerIconControl key={profile.id} profileId={profile.id} remote={remote} running={state !== "offline"} />
  </section>;
}
