import { useEffect, useState } from "react";
import { RefreshCw, UserRound } from "lucide-react";
import { loadPlayers } from "../lib/serverPlayers";
import type { PlayerRecord } from "../lib/serverPlayers";
import { readLivePlayerLocations, readSavedPlayerLocations } from "../lib/playerLocations";
import type { LivePlayerLocation } from "../lib/playerLocations";

type SavedLocation = { position: [number, number, number]; dimension: string | null };
type MapPoint = {
  key: string;
  name: string;
  uuid: string | null;
  position: [number, number, number];
  dimension: string;
  online: boolean;
  live: boolean;
};

const unknownDimension = "unknown";

function dimensionLabel(value: string) {
  if (value === unknownDimension) return "Unknown dimension";
  const name = value.split(":").pop();
  return name ? name.replace(/_/g, " ").replace(/\b\w/g, (letter) => letter.toUpperCase()) : value;
}

function coords(position: [number, number, number]) {
  return position.map((value) => Math.round(value).toLocaleString()).join(", ");
}

export function PlayerMapPanel({ profileId, remote, running }: { profileId: string; remote: boolean; running: boolean }) {
  const [players, setPlayers] = useState<PlayerRecord[]>([]);
  const [saved, setSaved] = useState<Record<string, SavedLocation>>({});
  const [live, setLive] = useState<Record<string, LivePlayerLocation>>({});
  const [dimension, setDimension] = useState("minecraft:overworld");
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(true);
  const [savedLoading, setSavedLoading] = useState(false);
  const [warning, setWarning] = useState<string | null>(null);
  const [savedWarning, setSavedWarning] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);

  useEffect(() => {
    let active = true;
    let timer: number | undefined;
    const requested = new Set<string>();
    const pending = new Set<string>();
    let readingSaved = false;
    let previousOnline = new Set<string>();
    let rosterShown = false;

    async function drainSaved() {
      if (!active || readingSaved || !pending.size) return;
      readingSaved = true;
      const uuids = [...pending].slice(0, 1000);
      for (const uuid of uuids) pending.delete(uuid);
      try {
        const locations = await readSavedPlayerLocations(remote, profileId, uuids);
        if (active) {
          setSaved((current) => {
            const next = { ...current };
            for (const location of locations) {
              if (location.position?.every(Number.isFinite)) next[location.uuid] = { position: location.position, dimension: location.dimension ?? null };
            }
            return next;
          });
          setSavedWarning(null);
        }
      } catch {
        for (const uuid of uuids) requested.delete(uuid);
        if (active) setSavedWarning("Could not read some saved player locations.");
      } finally {
        readingSaved = false;
        if (pending.size) void drainSaved();
        else if (active) setSavedLoading(false);
      }
    }

    function queueSaved(roster: PlayerRecord[]) {
      for (const player of roster) {
        if (player.uuid && !requested.has(player.uuid)) {
          requested.add(player.uuid);
          pending.add(player.uuid);
        }
      }
      if (pending.size) setSavedLoading(true);
      void drainSaved();
    }

    async function refresh() {
      setRefreshing(true);
      try {
        const result = await loadPlayers(remote, profileId, running, (roster) => {
          if (active) { if (!rosterShown) setPlayers(roster); queueSaved(roster); }
        });
        if (!active) return;
        rosterShown = true;
        setPlayers(result.players);
        for (const player of result.players) {
          if (player.uuid && !player.online && previousOnline.has(player.name?.toLowerCase() ?? "")) requested.delete(player.uuid);
        }
        queueSaved(result.players);
        previousOnline = new Set(result.players.filter((player) => player.online && player.name).map((player) => player.name!.toLowerCase()));
        let liveWarning: string | null = null;
        if (running && previousOnline.size) {
          try {
            const locations = await readLivePlayerLocations(remote, profileId, [...previousOnline]);
            if (active) setLive(Object.fromEntries(locations.map((location) => [location.name.toLowerCase(), location])));
            if (previousOnline.size && !locations.length) liveWarning = "Live positions are unavailable; showing saved positions where possible.";
          } catch {
            if (active) setLive({});
            liveWarning = "Could not read live positions; showing saved positions where possible.";
          }
        } else if (active) setLive({});
        if (active) setWarning([result.warning, liveWarning].filter(Boolean).join(" ") || null);
      } catch {
        if (active) {
          setLive({});
          setWarning("Could not refresh the player roster. Try Refresh.");
        }
      } finally {
        if (active) {
          setRefreshing(false);
          if (running) timer = window.setTimeout(() => void refresh(), 30000);
        }
      }
    }

    void refresh();
    return () => { active = false; if (timer !== undefined) window.clearTimeout(timer); };
  }, [profileId, remote, running, revision]);

  const points: MapPoint[] = [];
  const seenNames = new Set<string>();
  for (const player of players) {
    const name = player.name ?? player.uuid ?? "Unknown player";
    const liveLocation = player.name ? live[player.name.toLowerCase()] : undefined;
    const savedLocation = player.uuid ? saved[player.uuid] : undefined;
    const position = liveLocation?.position ?? savedLocation?.position;
    if (!position) continue;
    const online = player.online || !!liveLocation;
    points.push({ key: player.key, name, uuid: player.uuid, position,
      dimension: (liveLocation ? liveLocation.dimension : savedLocation?.dimension) ?? unknownDimension,
      online, live: !!liveLocation });
    if (player.name) seenNames.add(player.name.toLowerCase());
  }
  for (const location of Object.values(live)) {
    if (!seenNames.has(location.name.toLowerCase())) points.push({ key: location.name.toLowerCase(), name: location.name, uuid: null,
      position: location.position, dimension: location.dimension ?? unknownDimension, online: true, live: true });
  }
  const dimensionOrder = ["minecraft:overworld", "minecraft:the_nether", "minecraft:the_end"];
  const dimensions = [...new Set(points.map((point) => point.dimension))].sort((left, right) =>
    (dimensionOrder.indexOf(left) < 0 ? left === unknownDimension ? 4 : 3 : dimensionOrder.indexOf(left)) -
    (dimensionOrder.indexOf(right) < 0 ? right === unknownDimension ? 4 : 3 : dimensionOrder.indexOf(right)));
  const activeDimension = dimensions.includes(dimension) ? dimension : dimensions[0] ?? dimension;
  const visible = points.filter((point) => point.dimension === activeDimension);
  const selected = visible.find((point) => point.key === selectedKey) ?? null;
  const xs = visible.map((point) => point.position[0]);
  const zs = visible.map((point) => point.position[2]);
  const centerX = xs.length ? (Math.min(...xs) + Math.max(...xs)) / 2 : 0;
  const centerZ = zs.length ? (Math.min(...zs) + Math.max(...zs)) / 2 : 0;
  const span = Math.max(256, (xs.length ? Math.max(...xs) - Math.min(...xs) : 0) * 1.35, (zs.length ? Math.max(...zs) - Math.min(...zs) : 0) * 1.35);
  const minX = centerX - span / 2;
  const minZ = centerZ - span / 2;

  return <div className="player-map-panel">
    <header className="player-map-header">
      <div><h2>Player map</h2><p>Player positions on an X/Z coordinate grid. Offline markers show the last saved location.</p></div>
      <button className="button button-quiet" type="button" onClick={() => setRevision((value) => value + 1)} disabled={refreshing}><RefreshCw size={15} aria-hidden="true" /> {refreshing ? "Refreshing…" : "Refresh"}</button>
    </header>
    {warning && <p className="notice notice-error" role="status">{warning}</p>}
    {savedWarning && <p className="notice notice-error" role="status">{savedWarning}</p>}
    <div className="player-map-dimensions" aria-label="Dimension">
      {(dimensions.length ? dimensions : ["minecraft:overworld"]).map((item) => <button key={item} type="button" className={item === activeDimension ? "player-map-dimension active" : "player-map-dimension"} onClick={() => { setDimension(item); setSelectedKey(null); }}>{dimensionLabel(item)} <span>{points.filter((point) => point.dimension === item).length}</span></button>)}
    </div>
    <div className="player-map-layout">
      <div className="player-map-grid" role="group" aria-label={`Player coordinate grid for ${dimensionLabel(activeDimension)}`}>
        <span className="player-map-north">North · −Z</span><span className="player-map-east">East · +X</span>
        <span className="player-map-axis player-map-axis-x">X {Math.round(minX).toLocaleString()} → {Math.round(minX + span).toLocaleString()}</span>
        <span className="player-map-axis player-map-axis-z">Z {Math.round(minZ).toLocaleString()} → {Math.round(minZ + span).toLocaleString()}</span>
        {visible.map((point) => <button key={point.key} type="button" className={`player-map-marker${point.online ? "" : " player-map-marker-offline"}${selected?.key === point.key ? " player-map-marker-selected" : ""}`}
          style={{ left: `${Math.max(5, Math.min(95, 100 * (point.position[0] - minX) / span))}%`, top: `${Math.max(5, Math.min(95, 100 * (point.position[2] - minZ) / span))}%` }}
          aria-label={`${point.name}, ${point.online ? "online" : "offline"}, ${point.live ? "live" : "last saved"} position ${coords(point.position)}`}
          title={`${point.name} · ${coords(point.position)} · ${point.live ? "Live" : "Last saved"}`}
          onClick={() => setSelectedKey(point.key)}>
          <span className="player-map-head"><UserRound size={21} aria-hidden="true" /><img src={`https://mc-heads.net/avatar/${encodeURIComponent(point.uuid ?? point.name)}/64`} alt="" onError={(event) => { event.currentTarget.style.display = "none"; }} /></span>
          <span className="player-map-name">{point.name}</span>
        </button>)}
        {!visible.length && <p className="player-map-empty">{refreshing || savedLoading ? "Finding player locations…" : "No saved player locations in this dimension."}</p>}
      </div>
      <aside className="player-map-list" aria-label="Players on this map">
        <h3>Players <span>{visible.length}</span></h3>
        {visible.map((point) => <button key={point.key} type="button" className={selected?.key === point.key ? "player-map-list-item active" : "player-map-list-item"} onClick={() => setSelectedKey(point.key)}>
          <strong>{point.name}</strong><span>{point.online ? "Online" : "Offline"} · {point.live ? "Live" : "Last saved"}</span>
        </button>)}
        {selected && <div className="player-map-selection"><strong>{selected.name}</strong><span>X {selected.position[0].toFixed(1)} · Y {selected.position[1].toFixed(1)} · Z {selected.position[2].toFixed(1)}</span><small>{selected.live ? "Live console position" : "Last saved player file"}</small></div>}
      </aside>
    </div>
  </div>;
}
