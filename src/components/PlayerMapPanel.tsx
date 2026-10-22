import { useEffect, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent, KeyboardEvent as ReactKeyboardEvent } from "react";
import { Maximize2, Minus, Plus, RefreshCw, UserRound } from "lucide-react";
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
type MapView = { dimension: string; centerX: number; centerZ: number; unitsPerPixel: number };
type MapDrag = { pointerId: number; startX: number; startY: number; view: MapView };

const unknownDimension = "unknown";
const minimumZoom = 0.05;
const maximumZoom = 1_000_000;

function gridStep(unitsPerPixel: number) {
  const target = unitsPerPixel * 96;
  return 16 * 2 ** Math.max(0, Math.ceil(Math.log2(target / 16)));
}

function gridCoordinates(center: number, size: number, unitsPerPixel: number, step: number) {
  const first = Math.ceil((center - size * unitsPerPixel / 2) / step) * step;
  const count = Math.ceil(size * unitsPerPixel / step) + 1;
  return Array.from({ length: count }, (_, index) => first + index * step);
}

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
  const [view, setView] = useState<MapView | null>(null);
  const [gridSize, setGridSize] = useState({ width: 600, height: 420 });
  const [dragging, setDragging] = useState(false);
  const gridRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<MapDrag | null>(null);
  const viewRef = useRef<MapView | null>(null);

  useEffect(() => {
    const element = gridRef.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => {
      if (entry) setGridSize({ width: entry.contentRect.width, height: entry.contentRect.height });
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const element = gridRef.current;
    if (!element) return;
    const onWheel = (event: WheelEvent) => {
      const current = viewRef.current;
      if (!current || event.deltaY === 0) return;
      event.preventDefault();
      const rect = element.getBoundingClientRect();
      const offsetX = event.clientX - rect.left - rect.width / 2;
      const offsetY = event.clientY - rect.top - rect.height / 2;
      const nextUnits = Math.max(minimumZoom, Math.min(maximumZoom, current.unitsPerPixel * Math.exp(Math.max(-300, Math.min(300, event.deltaY)) * 0.001)));
      setView({ ...current, unitsPerPixel: nextUnits,
        centerX: current.centerX + offsetX * (current.unitsPerPixel - nextUnits),
        centerZ: current.centerZ + offsetY * (current.unitsPerPixel - nextUnits) });
    };
    element.addEventListener("wheel", onWheel, { passive: false });
    return () => element.removeEventListener("wheel", onWheel);
  }, []);

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
  const width = Math.max(gridSize.width, 1);
  const height = Math.max(gridSize.height, 1);
  const autoView: MapView = { dimension: activeDimension, centerX, centerZ,
    unitsPerPixel: Math.max(256 / Math.min(width, height),
      (xs.length ? Math.max(...xs) - Math.min(...xs) : 0) / (width * 0.75),
      (zs.length ? Math.max(...zs) - Math.min(...zs) : 0) / (height * 0.75)) };
  const currentView = view?.dimension === activeDimension ? view : autoView;
  viewRef.current = currentView;
  const step = gridStep(currentView.unitsPerPixel);
  const xLines = gridCoordinates(currentView.centerX, width, currentView.unitsPerPixel, step);
  const zLines = gridCoordinates(currentView.centerZ, height, currentView.unitsPerPixel, step);
  const screenX = (x: number) => width / 2 + (x - currentView.centerX) / currentView.unitsPerPixel;
  const screenY = (z: number) => height / 2 + (z - currentView.centerZ) / currentView.unitsPerPixel;
  const markers = visible.filter((point) => Math.abs(screenX(point.position[0]) - width / 2) < width / 2 + 65 &&
    Math.abs(screenY(point.position[2]) - height / 2) < height / 2 + 65);

  function zoom(factor: number) {
    setView({ ...currentView, unitsPerPixel: Math.max(minimumZoom, Math.min(maximumZoom, currentView.unitsPerPixel * factor)) });
  }

  function beginDrag(event: ReactPointerEvent<HTMLDivElement>) {
    if (event.button !== 0 || (event.target as HTMLElement).closest("button")) return;
    dragRef.current = { pointerId: event.pointerId, startX: event.clientX, startY: event.clientY, view: currentView };
    event.currentTarget.setPointerCapture(event.pointerId);
    setDragging(true);
  }

  function moveDrag(event: ReactPointerEvent<HTMLDivElement>) {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    setView({ ...drag.view,
      centerX: drag.view.centerX - (event.clientX - drag.startX) * drag.view.unitsPerPixel,
      centerZ: drag.view.centerZ - (event.clientY - drag.startY) * drag.view.unitsPerPixel });
  }

  function endDrag(event: ReactPointerEvent<HTMLDivElement>) {
    if (dragRef.current?.pointerId !== event.pointerId) return;
    dragRef.current = null;
    setDragging(false);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
  }

  function moveWithKeyboard(event: ReactKeyboardEvent<HTMLDivElement>) {
    if (event.target !== event.currentTarget) return;
    const offset = currentView.unitsPerPixel * 80;
    if (event.key === "ArrowLeft") setView({ ...currentView, centerX: currentView.centerX - offset });
    else if (event.key === "ArrowRight") setView({ ...currentView, centerX: currentView.centerX + offset });
    else if (event.key === "ArrowUp") setView({ ...currentView, centerZ: currentView.centerZ - offset });
    else if (event.key === "ArrowDown") setView({ ...currentView, centerZ: currentView.centerZ + offset });
    else if (event.key === "+" || event.key === "=") zoom(0.8);
    else if (event.key === "-") zoom(1.25);
    else if (event.key === "0") setView(null);
    else return;
    event.preventDefault();
  }

  return <div className="player-map-panel">
    <header className="player-map-header">
      <div><div className="player-map-title"><h2>Player map</h2><span className="player-map-progress">In Progress</span></div><p>Drag to move, scroll to zoom. The grid follows chunk boundaries; offline markers show the last saved location.</p></div>
      <button className="button button-quiet" type="button" onClick={() => setRevision((value) => value + 1)} disabled={refreshing}><RefreshCw size={15} aria-hidden="true" /> {refreshing ? "Refreshing…" : "Refresh"}</button>
    </header>
    {warning && <p className="notice notice-error" role="status">{warning}</p>}
    {savedWarning && <p className="notice notice-error" role="status">{savedWarning}</p>}
    <div className="player-map-dimensions" aria-label="Dimension">
      {(dimensions.length ? dimensions : ["minecraft:overworld"]).map((item) => <button key={item} type="button" className={item === activeDimension ? "player-map-dimension active" : "player-map-dimension"} onClick={() => { setDimension(item); setSelectedKey(null); }}>{dimensionLabel(item)} <span>{points.filter((point) => point.dimension === item).length}</span></button>)}
    </div>
    <div className="player-map-layout">
      <div ref={gridRef} className={`player-map-grid${dragging ? " player-map-grid-dragging" : ""}`} role="group" tabIndex={0}
        aria-label={`Player coordinate grid for ${dimensionLabel(activeDimension)}. Drag or use arrow keys to move; scroll or use the zoom buttons to zoom.`}
        onPointerDown={beginDrag} onPointerMove={moveDrag} onPointerUp={endDrag} onPointerCancel={endDrag} onKeyDown={moveWithKeyboard}>
        {xLines.map((x) => <span key={`x-${x}`} className={`player-map-line player-map-line-x${x === 0 ? " player-map-line-origin" : ""}`} style={{ left: screenX(x) }}><span>{Math.round(x).toLocaleString()}</span></span>)}
        {zLines.map((z) => <span key={`z-${z}`} className={`player-map-line player-map-line-z${z === 0 ? " player-map-line-origin" : ""}`} style={{ top: screenY(z) }}><span>{Math.round(z).toLocaleString()}</span></span>)}
        <span className="player-map-north">North · −Z</span><span className="player-map-east">East · +X</span>
        <div className="player-map-controls" aria-label="Map controls">
          <button type="button" aria-label="Zoom in" title="Zoom in" onClick={() => zoom(0.8)}><Plus size={16} aria-hidden="true" /></button>
          <button type="button" aria-label="Zoom out" title="Zoom out" onClick={() => zoom(1.25)}><Minus size={16} aria-hidden="true" /></button>
          <button type="button" aria-label="Fit all players" title="Fit all players" onClick={() => setView(null)}><Maximize2 size={15} aria-hidden="true" /></button>
        </div>
        <span className="player-map-scale">{(step / 16).toLocaleString()} {(step / 16) === 1 ? "chunk" : "chunks"} / grid · {step.toLocaleString()} blocks</span>
        {markers.map((point) => <button key={point.key} type="button" className={`player-map-marker${point.online ? "" : " player-map-marker-offline"}${selected?.key === point.key ? " player-map-marker-selected" : ""}`}
          style={{ left: screenX(point.position[0]), top: screenY(point.position[2]) }}
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
        {visible.map((point) => <button key={point.key} type="button" className={selected?.key === point.key ? "player-map-list-item active" : "player-map-list-item"} onClick={() => { setSelectedKey(point.key); setView({ ...currentView, centerX: point.position[0], centerZ: point.position[2] }); }}>
          <strong>{point.name}</strong><span>{point.online ? "Online" : "Offline"} · {point.live ? "Live" : "Last saved"}</span>
        </button>)}
        {selected && <div className="player-map-selection"><strong>{selected.name}</strong><span>X {selected.position[0].toFixed(1)} · Y {selected.position[1].toFixed(1)} · Z {selected.position[2].toFixed(1)}</span><small>{selected.live ? "Live console position" : "Last saved player file"}</small></div>}
      </aside>
    </div>
  </div>;
}
