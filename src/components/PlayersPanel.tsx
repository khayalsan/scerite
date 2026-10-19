import { useEffect, useState } from "react";
import { ArrowLeft, RefreshCw, UserRound } from "lucide-react";
import { serverSessions } from "../lib/serverSessions";
import { loadPlayers, playerData, validIpAddress, validPlayerName } from "../lib/serverPlayers";
import type { AccessLists, PlayerData, PlayerRecord, PlayerStatistic } from "../lib/serverPlayers";

interface PlayersPanelProps { profileId: string; remote: boolean; running: boolean }
type ManagementView = keyof AccessLists;
type View = "players" | ManagementView;

const listSettings: Record<ManagementView, { title: string; add: string; remove: string; byIp: boolean }> = {
  whitelist: { title: "Whitelist", add: "whitelist add", remove: "whitelist remove", byIp: false },
  operators: { title: "Operators", add: "op", remove: "deop", byIp: false },
  bannedNames: { title: "Banned names", add: "ban", remove: "pardon", byIp: false },
  bannedIps: { title: "Banned IPs", add: "ban-ip", remove: "pardon-ip", byIp: true },
};

const emptyLists: AccessLists = { whitelist: [], operators: [], bannedNames: [], bannedIps: [] };

function errorText(error: unknown) { return error instanceof Error ? error.message : String(error); }
function label(value: string) { return value.replace(/^minecraft:/, "").replace(/_/g, " "); }
function stat(stats: PlayerStatistic[], category: string, key: string) {
  return stats.find((entry) => entry.category === category && entry.key === key)?.value;
}
function formatPlaytime(ticks: number | undefined) {
  if (ticks === undefined) return "—";
  const minutes = Math.floor(ticks / 1200);
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

export function PlayersPanel({ profileId, remote, running }: PlayersPanelProps) {
  const [players, setPlayers] = useState<PlayerRecord[]>([]);
  const [lists, setLists] = useState<AccessLists>(emptyLists);
  const [view, setView] = useState<View>("players");
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [detail, setDetail] = useState<PlayerData | null>(null);
  const [entryInput, setEntryInput] = useState("");
  const [loading, setLoading] = useState(true);
  const [detailLoading, setDetailLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [warning, setWarning] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);

  useEffect(() => {
    let active = true;
    setLoading(true);
    void loadPlayers(remote, profileId, running).then((result) => {
      if (active) { setPlayers(result.players); setLists(result.lists); setWarning(result.warning); }
    }).catch((reason: unknown) => {
      if (active) setWarning(`Could not read the player list: ${errorText(reason)}`);
    }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [profileId, remote, running, revision]);

  const selected = players.find((player) => player.key === selectedKey) ?? null;
  useEffect(() => {
    if (!selected?.uuid) { setDetail(null); return; }
    let active = true;
    setDetailLoading(true);
    setDetail(null);
    void playerData(remote, profileId, selected.uuid).then((result) => {
      if (active) setDetail(result);
    }).catch((reason: unknown) => {
      if (active) setMessage(`Could not read saved player data: ${errorText(reason)}`);
    }).finally(() => { if (active) setDetailLoading(false); });
    return () => { active = false; };
  }, [profileId, remote, selected?.uuid, revision]);

  async function send(command: string, destructive = false) {
    if (!running) { setMessage("Start the server before changing player access."); return; }
    if (destructive && !window.confirm(`Send “${command}” to the server?`)) return;
    setBusy(true);
    setMessage(null);
    try {
      if (remote) await serverSessions.sendRemoteInput(profileId, command);
      else await serverSessions.sendInput(profileId, command);
      setMessage(`Sent “${command}”. Refresh after the server saves its player files to see the updated status.`);
      window.setTimeout(() => setRevision((value) => value + 1), 1500);
    } catch (reason) { setMessage(`Could not send the command: ${errorText(reason)}`); }
    finally { setBusy(false); }
  }

  function openList(next: ManagementView) {
    setView(next);
    setSelectedKey(null);
    setEntryInput("");
    setMessage(null);
  }

  function changeEntry(action: "add" | "remove", value: string) {
    if (view === "players") return;
    const setting = listSettings[view];
    const entry = value.trim();
    if (!(setting.byIp ? validIpAddress(entry) : validPlayerName(entry))) {
      setMessage(setting.byIp ? "Enter a valid IPv4 or IPv6 address." : "Enter a Minecraft player name (3–16 letters, numbers or underscores).");
      return;
    }
    void send(`${setting[action]} ${entry}`, action === "add" && (view === "bannedNames" || view === "bannedIps"));
  }

  const actionDisabled = !running || busy;
  const summary = detail?.statistics ?? [];
  const management = view === "players" ? null : listSettings[view];
  const entries = view === "players" ? [] : lists[view];

  return <div className="players-panel">
    <header className="players-header">
      <div><h2>Players</h2><p>Known players from server files. Online status is estimated from recent join and leave messages in latest.log.</p></div>
      <button className="button button-quiet" type="button" onClick={() => setRevision((value) => value + 1)} disabled={loading}><RefreshCw size={14} aria-hidden="true" /> Refresh</button>
    </header>
    {warning && <p className="notice notice-error" role="status">{warning}</p>}
    {message && <p className="notice" role="status">{message}</p>}
    {management ? <>
      <button className="button button-quiet players-back" type="button" onClick={() => { setView("players"); setMessage(null); }}><ArrowLeft size={16} /> All players</button>
      <section className="player-management" aria-label={management.title}>
        <div className="player-roster-heading"><h3>{management.title}</h3><span>{entries.length} entries</span></div>
        <form className="player-management-add" onSubmit={(event) => { event.preventDefault(); changeEntry("add", entryInput); }}>
          <label htmlFor="player-management-entry">{management.byIp ? "IP address" : "Player name"}</label>
          <div><input id="player-management-entry" value={entryInput} onChange={(event) => setEntryInput(event.currentTarget.value)} placeholder={management.byIp ? "IP address" : "Player name"} maxLength={management.byIp ? 39 : 16} />
            <button className="button button-primary" type="submit" disabled={actionDisabled || !entryInput.trim()}>Add</button></div>
        </form>
        {!running && <p className="players-caption">Start the server to add or remove entries.</p>}
        {loading ? <p className="state-message">Loading {management.title.toLowerCase()}…</p> : entries.length ?
          <div className="player-access-list">{entries.map((entry, index) => <div key={`${entry.value}-${index}`}>
            <span><strong>{entry.value}</strong>{entry.reason && <small>{entry.reason}</small>}</span>
            <button className="button button-quiet" type="button" disabled={actionDisabled || !(management.byIp ? validIpAddress(entry.value) : validPlayerName(entry.value))} onClick={() => changeEntry("remove", entry.value)}>Remove</button>
          </div>)}</div> : <p className="state-message">No entries in {management.title.toLowerCase()}.</p>}
      </section>
    </> : selected ? <>
      <button className="button button-quiet players-back" type="button" onClick={() => { setSelectedKey(null); setDetail(null); }}><ArrowLeft size={16} /> All players</button>
      <section className="player-detail" aria-label={`${selected.name ?? selected.uuid} details`}>
        <div className="player-identity">
          {selected.uuid ? <img className="player-head" src={`https://crafatar.com/avatars/${selected.uuid}?size=80&overlay`} alt="" onError={(event) => { event.currentTarget.style.display = "none"; }} /> : <UserRound size={48} aria-hidden="true" />}
          <div><h3>{selected.name ?? "Unknown player"}</h3><span className="player-uuid">{selected.uuid ?? "UUID unavailable"}</span><div className="player-badges"><span>{selected.online ? "Online (estimated)" : "Offline / not seen online"}</span><span>{selected.whitelisted ? "Whitelisted" : "Not whitelisted"}</span><span>{selected.operator ? "Operator" : "Not operator"}</span><span>{selected.banned ? "Banned" : "Not banned"}</span></div></div>
        </div>
        {detailLoading ? <p className="state-message">Reading saved player data…</p> : <>
          <p className="players-caption">Game details are from the last saved player file and may lag behind a running server.</p>
          {detail?.dataWarning && <p className="notice notice-error">{detail.dataWarning}</p>}
          <div className="player-facts">
            <div><span>Gamemode</span><strong>{detail?.gamemode ?? "—"}</strong></div>
            <div><span>Level</span><strong>{detail?.level ?? "—"}</strong></div>
            <div><span>Health</span><strong>{detail?.health ?? "—"}</strong></div>
            <div><span>Hunger</span><strong>{detail?.hunger ?? "—"}</strong></div>
            <div><span>Position (X, Y, Z)</span><strong>{detail?.position?.map((coordinate) => coordinate.toFixed(1)).join(", ") ?? "—"}</strong></div>
          </div>
          <h4>Statistics</h4>
          {detail?.statsWarning && <p className="notice notice-error">{detail.statsWarning}</p>}
          <div className="player-facts player-stat-summary">
            <div><span>Playtime</span><strong>{formatPlaytime(stat(summary, "minecraft:custom", "minecraft:play_time") ?? stat(summary, "minecraft:custom", "minecraft:play_one_minute"))}</strong></div>
            <div><span>Player kills</span><strong>{stat(summary, "minecraft:custom", "minecraft:player_kills") ?? "—"}</strong></div>
            <div><span>Deaths</span><strong>{stat(summary, "minecraft:custom", "minecraft:deaths") ?? "—"}</strong></div>
          </div>
          {summary.length ? <div className="player-stats-list">{summary.map((item) => <div key={`${item.category}/${item.key}`}><span>{label(item.category)} · {label(item.key)}</span><strong>{item.value.toLocaleString()}</strong></div>)}</div> : <p className="state-message">No saved statistics are available for this player.</p>}
        </>}
      </section>
    </> : <>
      <section className="player-actions" aria-label="Player management">
        <h3>Manage access</h3>
        <div className="player-management-links">
          <button className="button button-secondary" type="button" onClick={() => openList("whitelist")}>Whitelist <span>{lists.whitelist.length}</span></button>
          <button className="button button-secondary" type="button" onClick={() => openList("operators")}>OPs <span>{lists.operators.length}</span></button>
          <button className="button button-secondary" type="button" onClick={() => openList("bannedNames")}>Ban by name <span>{lists.bannedNames.length}</span></button>
          <button className="button button-secondary" type="button" onClick={() => openList("bannedIps")}>Ban by IP <span>{lists.bannedIps.length}</span></button>
        </div>
      </section>
      <section className="player-roster" aria-label="Player list">
        <div className="player-roster-heading"><h3>Known players</h3><span>{players.filter((player) => player.online).length} online estimated · {players.length} total</span></div>
        {loading ? <p className="state-message">Loading players…</p> : players.length ? <div className="player-list">{players.map((player) => <button type="button" key={player.key} onClick={() => { setSelectedKey(player.key); setMessage(null); }}>
          <span className={`player-presence${player.online ? " player-presence-online" : ""}`} aria-hidden="true" />
          <span><strong>{player.name ?? "Unknown player"}</strong><small>{player.uuid ?? "Name from server log"}</small></span>
          <em>{player.online ? "Online" : "Offline"}</em>
        </button>)}</div> : <p className="state-message">No known players yet. Players appear after joining or being added to a server list.</p>}
      </section>
    </>}
  </div>;
}
