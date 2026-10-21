import { useEffect, useState } from "react";
import { ArrowLeft, RefreshCw, UserRound } from "lucide-react";
import { serverSessions } from "../lib/serverSessions";
import { loadPlayers, playerData, validIpAddress, validPlayerName } from "../lib/serverPlayers";
import type { AccessLists, PlayerData, PlayerRecord, PlayerStatistic } from "../lib/serverPlayers";
import { SearchField } from "./SearchField";

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
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(true);
  const [rosterRefreshing, setRosterRefreshing] = useState(true);
  const [rosterUnavailable, setRosterUnavailable] = useState(false);
  const [onlineLoading, setOnlineLoading] = useState(false);
  const [onlineUnavailable, setOnlineUnavailable] = useState(false);
  const [detailLoading, setDetailLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [warning, setWarning] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);

  useEffect(() => {
    let active = true;
    setLoading(true);
    setRosterRefreshing(true);
    setRosterUnavailable(false);
    setOnlineLoading(false);
    setOnlineUnavailable(false);
    setWarning(null);
    void loadPlayers(remote, profileId, running, (roster, accessLists, ready) => {
      if (active) { setPlayers(roster); setLists(accessLists); setLoading(false); setRosterRefreshing(!ready); setOnlineLoading(running); }
    }).then((result) => {
      if (active) { setPlayers(result.players); setLists(result.lists); setWarning(result.warning); setOnlineUnavailable(result.onlineUnavailable); }
    }).catch((reason: unknown) => {
      if (active) { setRosterUnavailable(true); setOnlineUnavailable(true); setWarning(`Could not refresh the player list: ${errorText(reason)}`); }
    }).finally(() => { if (active) { setLoading(false); setRosterRefreshing(false); setOnlineLoading(false); } });
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
    setSearch("");
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

  const actionDisabled = !running || busy || rosterRefreshing || rosterUnavailable;
  const summary = detail?.statistics ?? [];
  const management = view === "players" ? null : listSettings[view];
  const entries = view === "players" ? [] : lists[view];
  const normalizedSearch = search.trim().toLowerCase();
  const visiblePlayers = players.filter((player) => !normalizedSearch || [player.name ?? "", player.uuid ?? ""].some((value) => value.toLowerCase().includes(normalizedSearch)));
  const visibleEntries = entries.filter((entry) => !normalizedSearch || [entry.value, entry.reason ?? ""].some((value) => value.toLowerCase().includes(normalizedSearch)));

  return <div className="players-panel">
    <header className="players-header">
      <div><h2>Players</h2><p>Known players from server files. Online status is checked with the server's list command when this page loads or refreshes.</p></div>
      <button className="button button-quiet" type="button" onClick={() => setRevision((value) => value + 1)} disabled={loading || rosterRefreshing || onlineLoading}><RefreshCw size={14} aria-hidden="true" /> Refresh</button>
    </header>
    {warning && <p className="notice notice-error" role="status">{warning}</p>}
    {message && <p className="notice" role="status">{message}</p>}
    {management ? <>
      <button className="button button-quiet players-back" type="button" onClick={() => { setView("players"); setSearch(""); setMessage(null); }}><ArrowLeft size={16} /> All players</button>
      <section className="player-management" aria-label={management.title}>
        <div className="player-roster-heading"><h3>{management.title}</h3><span>{rosterRefreshing ? "Refreshing…" : rosterUnavailable ? "Unavailable" : `${entries.length} entries`}</span></div>
        <form className="player-management-add" onSubmit={(event) => { event.preventDefault(); changeEntry("add", entryInput); }}>
          <label htmlFor="player-management-entry">{management.byIp ? "IP address" : "Player name"}</label>
          <div><input id="player-management-entry" value={entryInput} onChange={(event) => setEntryInput(event.currentTarget.value)} placeholder={management.byIp ? "IP address" : "Player name"} maxLength={management.byIp ? 39 : 16} />
            <button className="button button-primary" type="submit" disabled={actionDisabled || !entryInput.trim()}>Add</button></div>
        </form>
        {!running && <p className="players-caption">Start the server to add or remove entries.</p>}
        <SearchField id={`players-${view}-search`} label={`Search ${management.title.toLowerCase()}`} placeholder={management.byIp ? "IP address or reason" : "Player name or reason"} value={search} onChange={setSearch} />
        {rosterRefreshing || rosterUnavailable ? <p className="state-message">{rosterUnavailable ? "Could not refresh this list." : `Refreshing ${management.title.toLowerCase()}…`}</p> : visibleEntries.length ?
          <div className="player-access-list">{visibleEntries.map((entry, index) => <div key={`${entry.value}-${index}`}>
            <span><strong>{entry.value}</strong>{entry.reason && <small>{entry.reason}</small>}</span>
            <button className="button button-quiet" type="button" disabled={actionDisabled || !(management.byIp ? validIpAddress(entry.value) : validPlayerName(entry.value))} onClick={() => changeEntry("remove", entry.value)}>Remove</button>
          </div>)}</div> : <p className="state-message">{entries.length ? "No matching entries." : `No entries in ${management.title.toLowerCase()}.`}</p>}
      </section>
    </> : selected ? <>
      <button className="button button-quiet players-back" type="button" onClick={() => { setSelectedKey(null); setDetail(null); }}><ArrowLeft size={16} /> All players</button>
      <section className="player-detail" aria-label={`${selected.name ?? selected.uuid} details`}>
        <div className="player-identity">
          {selected.name || selected.uuid ? <img className="player-head" src={`https://mc-heads.net/avatar/${encodeURIComponent(selected.name ?? selected.uuid ?? "")}/80`} alt="" onError={(event) => { event.currentTarget.style.display = "none"; }} /> : <UserRound size={48} aria-hidden="true" />}
          <div><h3>{selected.name ?? "Unknown player"}</h3><span className="player-uuid">{selected.uuid ?? "UUID unavailable"}</span><div className="player-badges"><span>{onlineLoading ? "Checking online status…" : onlineUnavailable ? "Online status unavailable" : selected.online ? "Online (last checked)" : "Offline / not listed"}</span>{rosterRefreshing || rosterUnavailable ? <span>{rosterUnavailable ? "Access status unavailable" : "Checking access…"}</span> : <><span>{selected.whitelisted ? "Whitelisted" : "Not whitelisted"}</span><span>{selected.operator ? "Operator" : "Not operator"}</span><span>{selected.banned ? "Banned" : "Not banned"}</span></>}</div></div>
        </div>
        {detailLoading ? <p className="state-message">Reading saved player data…</p> : <>
          <p className="players-caption">Game details are from the last saved player file and may lag behind a running server.</p>
          {detail?.dataWarning && <p className="notice notice-error">{detail.dataWarning}</p>}
          {detail && !detail.hasSavedData && <p className="state-message">No saved player data yet. The server may not have saved this player since their first join. Refresh after it saves.</p>}
          {detail?.hasSavedData && <div className="player-facts">
            <div><span>Gamemode</span><strong>{detail?.gamemode ?? "—"}</strong></div>
            <div><span>Level</span><strong>{detail?.level ?? "—"}</strong></div>
            <div><span>Health</span><strong>{detail?.health ?? "—"}</strong></div>
            <div><span>Hunger</span><strong>{detail?.hunger ?? "—"}</strong></div>
            <div><span>Position (X, Y, Z)</span><strong>{detail?.position?.map((coordinate) => coordinate.toFixed(1)).join(", ") ?? "—"}</strong></div>
          </div>}
          <h4>Statistics</h4>
          {detail?.statsWarning && <p className="notice notice-error">{detail.statsWarning}</p>}
          {detail?.hasSavedStatistics && <div className="player-facts player-stat-summary">
            <div><span>Playtime</span><strong>{formatPlaytime(stat(summary, "minecraft:custom", "minecraft:play_time") ?? stat(summary, "minecraft:custom", "minecraft:play_one_minute"))}</strong></div>
            <div><span>Player kills</span><strong>{stat(summary, "minecraft:custom", "minecraft:player_kills") ?? "—"}</strong></div>
            <div><span>Deaths</span><strong>{stat(summary, "minecraft:custom", "minecraft:deaths") ?? "—"}</strong></div>
          </div>}
          {summary.length ? <div className="player-stats-list">{summary.map((item) => <div key={`${item.category}/${item.key}`}><span>{label(item.category)} · {label(item.key)}</span><strong>{item.value.toLocaleString()}</strong></div>)}</div> : <p className="state-message">{detail && !detail.hasSavedStatistics ? "No saved statistics yet. Refresh after the server saves this player." : "No saved statistics are available for this player."}</p>}
        </>}
      </section>
    </> : <>
      <section className="player-actions" aria-label="Player management">
        <h3>Manage access</h3>
        <div className="player-management-links">
          <button className="button button-secondary" type="button" onClick={() => openList("whitelist")}>Whitelist <span>{rosterRefreshing || rosterUnavailable ? "…" : lists.whitelist.length}</span></button>
          <button className="button button-secondary" type="button" onClick={() => openList("operators")}>OPs <span>{rosterRefreshing || rosterUnavailable ? "…" : lists.operators.length}</span></button>
          <button className="button button-secondary" type="button" onClick={() => openList("bannedNames")}>Ban by name <span>{rosterRefreshing || rosterUnavailable ? "…" : lists.bannedNames.length}</span></button>
          <button className="button button-secondary" type="button" onClick={() => openList("bannedIps")}>Ban by IP <span>{rosterRefreshing || rosterUnavailable ? "…" : lists.bannedIps.length}</span></button>
        </div>
      </section>
      <section className="player-roster" aria-label="Player list">
        <div className="player-roster-heading"><h3>Known players</h3><span>{onlineLoading ? "Checking online status…" : onlineUnavailable || rosterUnavailable ? "Online status unavailable" : `${players.filter((player) => player.online).length} online at last check`} · {players.length} {rosterRefreshing || rosterUnavailable ? "cached" : "total"}</span></div>
        <SearchField id="players-roster-search" label="Search players" placeholder="Name or UUID" value={search} onChange={setSearch} />
        {loading ? <p className="state-message">Loading players…</p> : visiblePlayers.length ? <div className="player-list">{visiblePlayers.map((player) => <button type="button" key={player.key} onClick={() => { setSelectedKey(player.key); setMessage(null); }}>
          <span className={`player-presence${player.online ? " player-presence-online" : ""}`} aria-hidden="true" />
          <span><strong>{player.name ?? "Unknown player"}</strong><small>{player.uuid ?? "Name from server log"}</small></span>
          <em>{onlineLoading ? "Checking…" : onlineUnavailable ? "Unknown" : player.online ? "Online" : "Offline"}</em>
        </button>)}</div> : <p className="state-message">{players.length ? "No matching players." : "No known players yet. Players appear after joining or being added to a server list."}</p>}
      </section>
    </>}
  </div>;
}
