import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ServerEnvironment, ServerProfile } from "../lib/serverProfiles";

type Mapping = { profileId: string; channelId: string; chat: boolean; status: boolean; joins: boolean };
type BridgeStatus = { enabled: boolean; running: boolean; hasToken: boolean; servers: Mapping[] };

function BridgeEnvironment({ environment, profiles }: { environment: ServerEnvironment; profiles: ServerProfile[] }) {
  const servers = profiles.filter((profile) => profile.target.kind === "remote" && profile.target.environmentId === environment.id);
  const [status, setStatus] = useState<BridgeStatus | null>(null);
  const [draft, setDraft] = useState<Mapping[]>([]);
  const [token, setToken] = useState("");
  const [editing, setEditing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    const next = await invoke<BridgeStatus>("discord_bridge_status", { environmentId: environment.id });
    setStatus(next);
    setDraft(servers.map((server) => next.servers.find((item) => item.profileId === server.id)
      ?? { profileId: server.id, channelId: "", chat: true, status: true, joins: true }));
  }

  useEffect(() => {
    let active = true;
    void invoke<BridgeStatus>("discord_bridge_status", { environmentId: environment.id })
      .then((next) => {
        if (!active) return;
        setStatus(next);
        setDraft(servers.map((server) => next.servers.find((item) => item.profileId === server.id)
          ?? { profileId: server.id, channelId: "", chat: true, status: true, joins: true }));
      })
      .catch((cause: unknown) => { if (active) setError(String(cause)); });
    return () => { active = false; };
  }, [environment.id, profiles]);

  function change(id: string, update: Partial<Mapping>) {
    setDraft((current) => current.map((item) => item.profileId === id ? { ...item, ...update } : item));
  }

  async function save() {
    setBusy(true);
    setError(null);
    try {
      await invoke("enable_discord_bridge", {
        environmentId: environment.id,
        token,
        servers: draft.filter((item) => item.channelId.trim()).map((item) => ({ ...item, channelId: item.channelId.trim() })),
      });
      setToken("");
      await refresh();
      setEditing(false);
    } catch (cause) {
      setError(String(cause));
      try { await refresh(); } catch { /* Keep the setup open when SSH is unavailable. */ }
    }
    finally { setBusy(false); }
  }

  async function disable() {
    setBusy(true);
    setError(null);
    try {
      await invoke("disable_discord_bridge", { environmentId: environment.id });
      setToken("");
      await refresh();
      setEditing(false);
    } catch (cause) { setError(String(cause)); }
    finally { setBusy(false); }
  }

  return <div className="discord-environment">
    <div className="app-setting-row">
      <span className="app-setting-copy"><strong>{environment.name}</strong><small>{!status ? "Checking bridge status…" : status.enabled ? status.running ? "Bridge running" : "Bridge configured but stopped" : !servers.length ? "Add a server here to set up Discord." : "Off — bridge not running"}</small></span>
      <label className="property-switch" aria-label={`Enable Discord for ${environment.name}`}>
        <input type="checkbox" checked={(status?.enabled ?? false) || editing} disabled={(!servers.length && !status?.enabled) || busy || status === null} onChange={(event) => {
          if (status?.enabled) void disable();
          else setEditing(event.currentTarget.checked);
        }} />
        <span aria-hidden="true" />
      </label>
    </div>
    {status?.enabled && !editing && <button className="button button-quiet" type="button" onClick={() => setEditing(true)}>Edit channels or bot token</button>}
    {editing && <div className="discord-setup">
      <p>Use a bot you own. Invite it to your Discord server, enable Message Content intent, and give it access to the channels below. Enable Discord Developer Mode to copy each channel ID. The token is stored on this remote environment.</p>
      <label className="field"><span>Bot token {status?.hasToken ? "(leave blank to keep the current token)" : ""}</span>
        <input type="password" autoComplete="off" value={token} onChange={(event) => setToken(event.currentTarget.value)} placeholder="Discord bot token" />
      </label>
      {servers.map((server) => {
        const value = draft.find((item) => item.profileId === server.id);
        if (!value) return null;
        return <div className="discord-server" key={server.id}>
          <strong>{server.name}</strong>
          <label className="field"><span>Channel ID (leave blank to exclude this server)</span><input inputMode="numeric" value={value.channelId} onChange={(event) => change(server.id, { channelId: event.currentTarget.value })} /></label>
          <div className="discord-options">
            <label><input type="checkbox" checked={value.chat} onChange={(event) => change(server.id, { chat: event.currentTarget.checked })} /> Chat both ways</label>
            <label><input type="checkbox" checked={value.status} onChange={(event) => change(server.id, { status: event.currentTarget.checked })} /> Online/offline</label>
            <label><input type="checkbox" checked={value.joins} onChange={(event) => change(server.id, { joins: event.currentTarget.checked })} /> Joins/leaves</label>
          </div>
        </div>;
      })}
      <div className="discord-actions">
        <button className="button button-primary" type="button" disabled={busy || (!token.trim() && !status?.hasToken) || !draft.some((item) => item.channelId.trim())} onClick={() => void save()}>{busy ? "Setting up…" : status?.enabled ? "Save Discord setup" : "Enable Discord"}</button>
        <button className="button button-quiet" type="button" disabled={busy} onClick={() => { setEditing(false); setToken(""); }}>Cancel</button>
      </div>
    </div>}
    {error && <p className="form-error" role="alert">{error}</p>}
  </div>;
}

export function DiscordBridgeSettings({ environments, profiles }: { environments: ServerEnvironment[]; profiles: ServerProfile[] }) {
  return <section className="app-settings-section" aria-labelledby="discord-settings-title">
    <div className="environment-section-heading"><h2 id="discord-settings-title">Discord bridge</h2></div>
    <p className="discord-intro">Optional chat and server notifications. Disabled by default. Scerite downloads the bridge only when you enable it. Turning it off stops the bot and removes its token and channel settings from that host.</p>
    {environments.length ? environments.map((environment) => <BridgeEnvironment key={environment.id} environment={environment} profiles={profiles} />)
      : <p className="discord-intro">Add a remote environment to set up Discord.</p>}
  </section>;
}
