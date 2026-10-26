import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ArrowLeft, ArrowRight, ExternalLink, Globe2, LockKeyhole, Route } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { ServerProfile } from "../lib/serverProfiles";
import { serverFiles } from "../lib/serverFiles";
import { velocityOnDemand } from "../lib/velocityOnDemand";
import { PlayitSetupPanel } from "./PlayitSetupPanel";

export type ConnectionMethod = "direct" | "tailscale" | "playit";

export const connectionMethods: { id: ConnectionMethod; title: string; summary: string; icon: LucideIcon }[] = [
  { id: "direct", title: "Direct", summary: "Public address · Router or cloud firewall setup", icon: Globe2 },
  { id: "tailscale", title: "Tailscale", summary: "Private invite · Friends use Tailscale", icon: LockKeyhole },
  { id: "playit", title: "Playit", summary: "Public address · No port forwarding", icon: Route },
];

export function ConnectionMethodBanners({ onSelect }: { onSelect: (method: ConnectionMethod) => void }) {
  return <section className="connection-methods" aria-labelledby="connection-methods-title">
    <h3 id="connection-methods-title">How players connect</h3>
    <div className="connection-method-banners">
      {connectionMethods.map(({ id, title, summary, icon: Icon }) => <button key={id} type="button" className="connection-method-banner" onClick={() => onSelect(id)}>
        <Icon size={21} aria-hidden="true" />
        <span><strong>{title}</strong><small>{summary}</small></span>
        <ArrowRight size={18} aria-hidden="true" />
      </button>)}
    </div>
  </section>;
}

export function ConnectionMethodPanel({ profile, method, onBack }: { profile: ServerProfile; method: ConnectionMethod; onBack: () => void }) {
  const remote = profile.target.kind === "remote";
  const [port, setPort] = useState<number | null>(null);
  const [portDraft, setPortDraft] = useState("");
  const [portError, setPortError] = useState<string | null>(null);
  const [reachability, setReachability] = useState<{ port: number; reachable: boolean } | null>(null);
  const [checkError, setCheckError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [guideError, setGuideError] = useState<string | null>(null);
  const checkRequest = useRef(0);

  useEffect(() => {
    let active = true;
    const read = async () => {
      try {
        const supportsVelocity = remote && ["paper", "purpur"].includes(profile.installedSoftware ?? "");
        const [properties, velocityResult] = await Promise.all([
          serverFiles.read(remote, profile.id, "server.properties"),
          supportsVelocity
            ? velocityOnDemand.status(profile.id).then((status) => ({ status, failed: false })).catch(() => ({ status: null, failed: true }))
            : Promise.resolve({ status: null, failed: false }),
        ]);
        if (velocityResult.failed) {
          if (active) setPortError("Could not read the proxy port. Check Velocity settings before using this port.");
          return;
        }
        const match = /^server-port\s*=\s*(\d+)\s*$/m.exec(properties);
        const configured = Number(match?.[1]);
        const next = velocityResult.status?.enabled ? velocityResult.status.publicPort : Number.isInteger(configured) && configured > 0 && configured <= 65535 ? configured : 25565;
        if (active) { setPort(next); setPortDraft(String(next)); }
      } catch {
        if (active) setPortError("Could not read the server port. Enter it to check or configure this method.");
      }
    };
    void read();
    return () => { active = false; checkRequest.current += 1; };
  }, [profile.id, profile.installedSoftware, remote]);

  async function checkPort() {
    const selectedPort = Number(portDraft);
    if (!Number.isInteger(selectedPort) || selectedPort < 1 || selectedPort > 65535) {
      setCheckError("Enter a port between 1 and 65535.");
      return;
    }
    const request = ++checkRequest.current;
    setChecking(true);
    setReachability(null);
    setCheckError(null);
    try {
      const reachable = await invoke<boolean>(remote ? "check_remote_public_port" : "check_local_public_port", remote ? { profileId: profile.id, port: selectedPort } : { port: selectedPort });
      if (request === checkRequest.current) setReachability({ port: selectedPort, reachable });
    } catch (reason) {
      if (request === checkRequest.current) setCheckError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      if (request === checkRequest.current) setChecking(false);
    }
  }

  function openGuide(url: string) {
    setGuideError(null);
    void openUrl(url).catch((reason: unknown) => setGuideError(reason instanceof Error ? reason.message : String(reason)));
  }

  const title = connectionMethods.find((option) => option.id === method)?.title ?? "Connection";
  return <section className="connection-method-page" aria-label={`${title} connection setup`}>
    <button type="button" className="connection-method-back" onClick={onBack}><ArrowLeft size={16} aria-hidden="true" /> Server</button>
    <header><h2>{title}</h2><p>{method === "direct" ? "Players connect through your public address." : method === "tailscale" ? "Only people you invite can reach this server." : "Players connect through a Playit address without router changes."}</p></header>

    {method === "direct" && <>
      <section className="server-reachability" aria-labelledby="server-reachability-title">
        <h3 id="server-reachability-title">Public port check</h3>
        <p>Check whether this {remote ? "remote host's" : "computer's"} public IP accepts TCP connections on a port.</p>
        <div className="server-reachability-actions">
          <label htmlFor="server-public-port">Port</label>
          <input id="server-public-port" type="number" min="1" max="65535" value={portDraft} onChange={(event) => { checkRequest.current += 1; setPortDraft(event.currentTarget.value); setReachability(null); setCheckError(null); setChecking(false); }} />
          <button className="button button-secondary" type="button" disabled={checking} onClick={() => void checkPort()}>{checking ? "Checking…" : "Check"}</button>
        </div>
        {reachability && <p role="status" className={reachability.reachable ? "server-reachability-success" : "server-reachability-failure"}>Port {reachability.port} is {reachability.reachable ? "reachable" : "not reachable"} from the public checker.</p>}
        {checkError && <p className="form-error" role="alert">{checkError}</p>}
        {portError && <p className="form-error" role="alert">{portError}</p>}
        <small>A reachable TCP port does not confirm Minecraft is accepting players.</small>
      </section>
      <div className="connection-method-steps">
        <h3>Set up direct access</h3>
        <ol>
          <li>Start the server and allow TCP traffic on {port === null ? "its game port" : `port ${port}`} in the host firewall.</li>
          <li>At home, forward a router port to this computer. On a cloud host, allow the port in its network firewall.</li>
          <li>Run the check above, then give players your public address and port.</li>
        </ol>
      </div>
      <button className="button button-secondary" type="button" onClick={() => openGuide("https://help.minecraft.net/hc/en-us/articles/360058525452-How-to-Setup-a-Minecraft-Java-Edition-Server")}>Minecraft server guide <ExternalLink size={15} aria-hidden="true" /></button>
    </>}

    {method === "tailscale" && <>
      <div className="connection-method-steps">
        <h3>Set up a private connection</h3>
        <ol>
          <li>Install Tailscale on the computer running this server and sign in.</li>
          <li>Share that machine with the friends who should join. Each friend installs Tailscale and accepts the invite.</li>
          <li>Give friends the server machine's Tailscale IP and {port === null ? "Minecraft port" : `port ${port}`}.</li>
        </ol>
      </div>
      {portError && <p className="form-error" role="alert">{portError}</p>}
      <button className="button button-secondary" type="button" onClick={() => openGuide("https://tailscale.com/docs/use-cases/personal-or-at-home-use/share-private-game-server")}>Tailscale guide <ExternalLink size={15} aria-hidden="true" /></button>
    </>}

    {method === "playit" && <>
      {remote && <PlayitSetupPanel profileId={profile.id} port={port} />}
      <div className="connection-method-steps">
        <h3>Set up a public tunnel</h3>
        <ol>
          <li>{remote ? "Set up the agent above and claim it in your Playit account." : "Install the Playit agent on the computer running this server and claim it in your Playit account."}</li>
          <li>Create a Minecraft Java tunnel pointing to {port === null ? "the server's listening port" : `port ${port}`} on that computer.</li>
          <li>Share the address Playit assigns. Keep the agent running so players can connect.</li>
        </ol>
      </div>
      {portError && <p className="form-error" role="alert">{portError}</p>}
      <button className="button button-secondary" type="button" onClick={() => openGuide("https://playit.gg/")}>Open Playit <ExternalLink size={15} aria-hidden="true" /></button>
    </>}
    {guideError && <p className="form-error" role="alert">Could not open the guide: {guideError}</p>}
  </section>;
}
