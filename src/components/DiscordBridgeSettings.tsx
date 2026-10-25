import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ServerEnvironment, ServerProfile } from "../lib/serverProfiles";

type Action = "chatToDiscord" | "chatToMinecraft" | "online" | "offline" | "join" | "leave" | "kills" | "achievements";
type Mapping = { profileId: string; channelId: string; chatToMinecraftTemplate: string } & Record<Action, boolean> & Record<`${Action}ChannelId`, string>;
const defaultMinecraftTemplate = "<🎮{username}> {message}";
type BridgeStatus = { enabled: boolean; running: boolean; hasToken: boolean; updateAvailable: boolean; servers: Mapping[] };
const actions: { key: Action; label: string }[] = [
  { key: "chatToDiscord", label: "Game chat → Discord" },
  { key: "chatToMinecraft", label: "Discord chat → game" },
  { key: "online", label: "Server online" },
  { key: "offline", label: "Server offline" },
  { key: "join", label: "Player joined" },
  { key: "leave", label: "Player left" },
  { key: "kills", label: "Player deaths (including PvP)" },
  { key: "achievements", label: "Advancement earned" },
];
function defaultMapping(profileId: string): Mapping {
  return { profileId, channelId: "", chatToMinecraftTemplate: defaultMinecraftTemplate, chatToDiscord: true, chatToMinecraft: true, online: true, offline: true, join: true, leave: true, kills: true, achievements: true,
    chatToDiscordChannelId: "", chatToMinecraftChannelId: "", onlineChannelId: "", offlineChannelId: "", joinChannelId: "", leaveChannelId: "", killsChannelId: "", achievementsChannelId: "" };
}

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
      ?? defaultMapping(server.id)));
  }

  useEffect(() => {
    let active = true;
    void invoke<BridgeStatus>("discord_bridge_status", { environmentId: environment.id })
      .then((next) => {
        if (!active) return;
        setStatus(next);
        setDraft(servers.map((server) => next.servers.find((item) => item.profileId === server.id)
          ?? defaultMapping(server.id)));
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
        servers: draft.map((item) => Object.fromEntries(Object.entries(item).map(([key, value]) => [key, key.endsWith("ChannelId") || key === "channelId" ? String(value).trim() : value]))),
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

  async function updateBot() {
    if (!status?.enabled) return;
    setBusy(true);
    setError(null);
    try {
      await invoke("enable_discord_bridge", { environmentId: environment.id, token: "", servers: status.servers });
      await refresh();
    } catch (cause) { setError(String(cause)); }
    finally { setBusy(false); }
  }

  return <div className="discord-environment">
    <div className="app-setting-row">
      <span className="app-setting-copy"><strong>{environment.name}</strong><small>{!status ? "Checking bridge status…" : status.enabled ? status.updateAvailable ? "Bot update available" : status.running ? "Bridge running · up to date" : "Bridge configured but stopped" : "Off — bridge not running"}</small></span>
      <label className="property-switch" aria-label={`Enable Discord for ${environment.name}`}>
        <input type="checkbox" checked={(status?.enabled ?? false) || editing} disabled={busy || status === null} onChange={(event) => {
          if (status?.enabled) void disable();
          else setEditing(event.currentTarget.checked);
        }} />
        <span aria-hidden="true" />
      </label>
    </div>
    {status?.enabled && !editing && <div className="discord-actions">
      <button className="button button-quiet" type="button" disabled={busy} onClick={() => setEditing(true)}>Edit channels or bot token</button>
      <button className="button button-secondary" type="button" disabled={busy || !status.updateAvailable} onClick={() => void updateBot()}>{busy ? "Updating bot…" : "Update bot"}</button>
    </div>}
    {editing && <div className="discord-setup">
      <p>Use a bot you own. Invite it to your Discord server, enable Message Content intent, and grant Manage Webhooks for player-style chat. You can add channel IDs now or later. Enable Discord Developer Mode to copy channel IDs. The token is stored on this remote environment.</p>
      <label className="field"><span>Bot token {status?.hasToken ? "(leave blank to keep the current token)" : ""}</span>
        <input type="password" autoComplete="off" value={token} onChange={(event) => setToken(event.currentTarget.value)} placeholder="Discord bot token" />
      </label>
      {servers.map((server) => {
        const value = draft.find((item) => item.profileId === server.id);
        if (!value) return null;
        return <div className="discord-server" key={server.id}>
          <strong>{server.name}</strong>
          <label className="field"><span>Default channel ID (optional)</span><input inputMode="numeric" value={value.channelId} onChange={(event) => change(server.id, { channelId: event.currentTarget.value })} /></label>
          <label className="field"><span>Discord → game message format</span>
            <input value={value.chatToMinecraftTemplate} maxLength={200} onChange={(event) => change(server.id, { chatToMinecraftTemplate: event.currentTarget.value })} />
            <small>Use {"{username}"} for the Discord display name and {"{message}"} for the message. Preview: {value.chatToMinecraftTemplate.replace(/\{username\}|\{message\}/g, (placeholder) => placeholder === "{username}" ? "Alex" : "Hello from Discord")}</small>
          </label>
          <div className="discord-event-list">
            {actions.map(({ key, label }) => {
              const channelKey = `${key}ChannelId` as const;
              return <div className="discord-event-row" key={key}>
                <label><input type="checkbox" checked={value[key]} onChange={(event) => change(server.id, { [key]: event.currentTarget.checked })} /> {label}</label>
                <input aria-label={`${label} channel ID for ${server.name}`} inputMode="numeric" placeholder="Default channel" value={value[channelKey]} disabled={!value[key]} onChange={(event) => change(server.id, { [channelKey]: event.currentTarget.value })} />
              </div>;
            })}
          </div>
        </div>;
      })}
      <div className="discord-actions">
        <button className="button button-primary" type="button" disabled={busy || (!token.trim() && !status?.hasToken)} onClick={() => void save()}>{busy ? "Setting up…" : status?.enabled ? "Save Discord setup" : "Enable Discord"}</button>
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
