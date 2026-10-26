import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

type PlayitStatus = { installed: boolean; enabled: boolean; running: boolean; claimed: boolean; claimUrl: string | null; claimSetupRunning: boolean; connected: boolean; needsAttention: boolean };

export function PlayitSetupPanel({ profileId, port }: { profileId: string; port: number | null }) {
  const [status, setStatus] = useState<PlayitStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    const refresh = async () => {
      try {
        const next = await invoke<PlayitStatus>("playit_agent_status", { profileId });
        if (active) { setStatus(next); setError(null); }
      } catch (cause) { if (active) setError(String(cause)); }
    };
    void refresh();
    return () => { active = false; };
  }, [profileId]);

  useEffect(() => {
    if (!status?.enabled || !status.running || status.connected || status.needsAttention || busy) return;
    const timer = window.setInterval(() => {
      void invoke<PlayitStatus>("playit_agent_status", { profileId }).then(setStatus).catch((cause: unknown) => setError(String(cause)));
    }, 3000);
    return () => window.clearInterval(timer);
  }, [profileId, status?.enabled, status?.running, status?.connected, status?.needsAttention, busy]);

  async function change(enable: boolean) {
    setBusy(true);
    setError(null);
    try {
      const next = await invoke<PlayitStatus>(enable ? "enable_playit_agent" : "disable_playit_agent", { profileId });
      setStatus(next);
    } catch (cause) { setError(String(cause)); }
    finally { setBusy(false); }
  }

  async function remove() {
    if (!window.confirm("Delete the Playit agent and saved account link from this remote environment? This disconnects Playit tunnels for every server here. Tunnels in your Playit account will remain.")) return;
    setBusy(true);
    setError(null);
    try { setStatus(await invoke<PlayitStatus>("delete_playit_agent", { profileId })); }
    catch (cause) { setError(String(cause)); }
    finally { setBusy(false); }
  }

  async function openClaim() {
    if (!status?.claimUrl || !/^https:\/\/playit\.gg\/claim\/[a-fA-F0-9]{1,32}$/.test(status.claimUrl)) return;
    try { await openUrl(status.claimUrl); }
    catch (cause) { setError(String(cause)); }
  }

  async function openTunnelSetup() {
    try { await openUrl("https://playit.gg/account/setup/new-tunnel"); }
    catch (cause) { setError(String(cause)); }
  }

  return <section className="playit-setup" aria-labelledby="playit-setup-title">
    <div className="playit-setup-heading">
      <div><h3 id="playit-setup-title">Playit agent</h3><p>One agent is shared by servers in this remote environment. Turning it off disconnects their tunnels.</p></div>
      <button className="button button-secondary" type="button" disabled={busy || status === null} onClick={() => void change(!status?.enabled || !status.running || !status.claimed && !status.claimSetupRunning)}>
        {busy ? "Working…" : !status?.enabled ? "Set up" : !status.running ? "Start again" : !status.claimed && !status.claimSetupRunning ? "Retry claim" : "Turn off"}
      </button>
    </div>
    {status?.installed && !status.enabled && <p role="status">Agent turned off. Its files and account link are still saved.</p>}
    {status?.enabled && <>
      <p role="status">{!status.running ? "Agent stopped." : status.needsAttention ? "Playit needs attention. Check the agent on the remote host." : status.connected ? "Agent connected to Playit." : status.claimed ? "Account linked. Connecting to Playit…" : "Agent running. Claim it in your Playit account."}</p>
      {status.claimUrl && <button className="button button-secondary" type="button" onClick={() => void openClaim()}>Claim agent</button>}
      {!status.claimed && !status.claimUrl && status.claimSetupRunning && <p>Preparing the claim link…</p>}
      {!status.claimed && !status.claimSetupRunning && <button className="button button-quiet" type="button" disabled={busy} onClick={() => void change(false)}>Turn off</button>}
      {status.claimed && <p>Create a Minecraft Java tunnel in your Playit dashboard. Set its local address to <code>127.0.0.1</code> and its local port to <strong>{port ?? "this server’s listening port"}</strong>. Share the address Playit assigns.</p>}
      {status.claimed && <button className="button button-secondary" type="button" onClick={() => void openTunnelSetup()}>Create tunnel</button>}
      <button className="button button-quiet" type="button" disabled={busy} onClick={() => void invoke<PlayitStatus>("playit_agent_status", { profileId }).then(setStatus).catch((cause: unknown) => setError(String(cause)))}>Refresh status</button>
    </>}
    {status?.installed && <button className="button button-danger-quiet" type="button" disabled={busy} onClick={() => void remove()}>Delete Playit files</button>}
    {error && <p className="form-error" role="alert">{error}</p>}
  </section>;
}
