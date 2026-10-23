import { useEffect, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent, KeyboardEvent as ReactKeyboardEvent } from "react";
import { Maximize2, Minus, Plus, RefreshCw, UserRound } from "lucide-react";
import { loadPlayers } from "../lib/serverPlayers";
import type { PlayerRecord } from "../lib/serverPlayers";
import { readLivePlayerLocations, readSavedPlayerLocations } from "../lib/playerLocations";
import type { LivePlayerLocation } from "../lib/playerLocations";
import { readTerrainTiles, TerrainTileCache, terrainResolution } from "../lib/terrainTiles";
import type { TerrainRequest, TileResolution } from "../lib/terrainTiles";
import { displayCache } from "../lib/displayCache";

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
type TerrainWork = { dimension: string; resolution: TileResolution; requests: TerrainRequest[]; generation: number; force: boolean };

const unknownDimension = "unknown";
const minimumZoom = 0.05;
const maximumZoom = 1_000_000;
const terrainPollMs = 45_000;
const terrainBatchSize = 16;
const terrainViewLimit = 512;
const terrainReadBudgetBytes = 32 * 1024 * 1024;

function gridStep(unitsPerPixel: number) {
  const target = unitsPerPixel * 96;
  return 16 * 2 ** Math.max(0, Math.ceil(Math.log2(target / 16)));
}

function gridCoordinates(center: number, size: number, unitsPerPixel: number, step: number) {
  const first = Math.ceil((center - size * unitsPerPixel / 2) / step) * step;
  const count = Math.ceil(size * unitsPerPixel / step) + 1;
  return Array.from({ length: count }, (_, index) => first + index * step);
}

function visibleChunks(view: MapView, width: number, height: number, limit: number, priority: [number, number][]): [number, number][] {
  const minX = Math.max(-2_000_000, Math.floor((view.centerX - width * view.unitsPerPixel / 2) / 16));
  const maxX = Math.min(2_000_000, Math.floor((view.centerX + width * view.unitsPerPixel / 2) / 16));
  const minZ = Math.max(-2_000_000, Math.floor((view.centerZ - height * view.unitsPerPixel / 2) / 16));
  const maxZ = Math.min(2_000_000, Math.floor((view.centerZ + height * view.unitsPerPixel / 2) / 16));
  const centerX = Math.max(minX, Math.min(maxX, Math.floor(view.centerX / 16)));
  const centerZ = Math.max(minZ, Math.min(maxZ, Math.floor(view.centerZ / 16)));
  const chunks: [number, number][] = [];
  const seen = new Set<string>();
  for (const [x, z] of priority) {
    const key = `${x}|${z}`;
    if (x >= minX && x <= maxX && z >= minZ && z <= maxZ && !seen.has(key) && chunks.length < limit) {
      chunks.push([x, z]);
      seen.add(key);
    }
  }
  const radius = Math.min(64, Math.max(maxX - centerX, centerX - minX, maxZ - centerZ, centerZ - minZ));
  for (let ring = 0; ring <= radius && chunks.length < limit; ring++) {
    for (let z = centerZ - ring; z <= centerZ + ring && chunks.length < limit; z++) {
      for (let x = centerX - ring; x <= centerX + ring && chunks.length < limit; x++) {
        if (Math.max(Math.abs(x - centerX), Math.abs(z - centerZ)) !== ring) continue;
        const key = `${x}|${z}`;
        if (x >= minX && x <= maxX && z >= minZ && z <= maxZ && !seen.has(key)) {
          chunks.push([x, z]);
          seen.add(key);
        }
      }
    }
  }
  return chunks;
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
  const [cacheReady, setCacheReady] = useState(!remote);
  const [gridSize, setGridSize] = useState({ width: 600, height: 420 });
  const [dragging, setDragging] = useState(false);
  const [terrainEpoch, setTerrainEpoch] = useState(0);
  const [terrainTick, setTerrainTick] = useState(0);
  const [terrainRefresh, setTerrainRefresh] = useState(0);
  const [terrainWarning, setTerrainWarning] = useState<string | null>(null);
  const gridRef = useRef<HTMLDivElement>(null);
  const terrainCanvasRef = useRef<HTMLCanvasElement>(null);
  const terrainCacheRef = useRef<TerrainTileCache | null>(null);
  const terrainQueueRef = useRef<TerrainWork | null>(null);
  const terrainBusyRef = useRef(false);
  const terrainGenerationRef = useRef(0);
  const terrainMountedRef = useRef(false);
  const lastTerrainRefreshRef = useRef(0);
  const dragRef = useRef<MapDrag | null>(null);
  const viewRef = useRef<MapView | null>(null);
  if (!terrainCacheRef.current) terrainCacheRef.current = new TerrainTileCache();

  useEffect(() => {
    terrainMountedRef.current = true;
    const timer = window.setInterval(() => setTerrainTick((tick) => tick + 1), terrainPollMs);
    return () => { terrainMountedRef.current = false; terrainQueueRef.current = null; terrainGenerationRef.current++; window.clearInterval(timer); };
  }, []);

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

    if (remote) {
      void displayCache.readMap(profileId).then((snapshot) => {
        if (!active || !snapshot || !Array.isArray(snapshot.players) || !snapshot.saved || typeof snapshot.saved !== "object") return;
        setPlayers(snapshot.players.filter((player) => player && typeof player.key === "string"
          && (player.name === null || typeof player.name === "string")
          && (player.uuid === null || typeof player.uuid === "string")).map((player) => ({ ...player, online: false })));
        setSaved(Object.fromEntries(Object.entries(snapshot.saved).filter(([, location]) => location
          && Array.isArray(location.position) && location.position.length === 3
          && location.position.every((value) => typeof value === "number" && Number.isFinite(value))
          && (location.dimension === null || typeof location.dimension === "string"))));
        if (snapshot.view && Number.isFinite(snapshot.view.centerX) && Number.isFinite(snapshot.view.centerZ)
          && Number.isFinite(snapshot.view.unitsPerPixel) && snapshot.view.unitsPerPixel >= minimumZoom
          && snapshot.view.unitsPerPixel <= maximumZoom && typeof snapshot.view.dimension === "string") {
          setDimension(snapshot.view.dimension);
          setView(snapshot.view);
        }
      }).catch(() => {}).finally(() => { if (active) { setCacheReady(true); void refresh(); } });
    } else void refresh();
    return () => { active = false; if (timer !== undefined) window.clearTimeout(timer); };
  }, [profileId, remote, running, revision]);

  useEffect(() => {
    if (!remote || !cacheReady) return;
    const timer = window.setTimeout(() => {
      void displayCache.writeMap(profileId, { players: players.map((player) => ({ ...player, online: false })), saved, view: viewRef.current }).catch(() => {});
    }, 800);
    return () => window.clearTimeout(timer);
  }, [remote, cacheReady, profileId, players, saved, view]);

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
  const priorityChunks = visible.map((point) => [Math.floor(point.position[0] / 16), Math.floor(point.position[2] / 16)] as [number, number]);
  const priorityKey = priorityChunks.map(([x, z]) => `${x}|${z}`).join(",");
  const visibleChunkCount = (Math.ceil((width * currentView.unitsPerPixel) / 16) + 1) * (Math.ceil((height * currentView.unitsPerPixel) / 16) + 1);

  useEffect(() => {
    const canvas = terrainCanvasRef.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context) return;
    const scale = Math.min(window.devicePixelRatio || 1, 2, Math.sqrt(2_000_000 / (width * height)));
    canvas.width = Math.ceil(width * scale);
    canvas.height = Math.ceil(height * scale);
    context.setTransform(scale, 0, 0, scale, 0, 0);
    context.clearRect(0, 0, width, height);
    context.imageSmoothingEnabled = false;
    const resolution = terrainResolution(currentView.unitsPerPixel);
    for (const [x, z] of terrainCacheRef.current!.coordinates(activeDimension)) {
      const left = width / 2 + (x * 16 - currentView.centerX) / currentView.unitsPerPixel;
      const top = height / 2 + (z * 16 - currentView.centerZ) / currentView.unitsPerPixel;
      const size = 16 / currentView.unitsPerPixel;
      if (left + size < 0 || top + size < 0 || left > width || top > height) continue;
      const tile = terrainCacheRef.current!.tile(activeDimension, x, z, resolution);
      if (tile?.canvas) context.drawImage(tile.canvas, left, top, size, size);
    }
  }, [activeDimension, currentView.centerX, currentView.centerZ, currentView.unitsPerPixel, width, height, terrainEpoch]);

  async function drainTerrain() {
    if (terrainBusyRef.current) return;
    terrainBusyRef.current = true;
    try {
      while (terrainMountedRef.current && terrainQueueRef.current) {
        const work = terrainQueueRef.current;
        terrainQueueRef.current = null;
        let hadError = false;
        let bytesRead = 0;
        let limited = false;
        for (let offset = 0; offset < work.requests.length && terrainMountedRef.current && work.generation === terrainGenerationRef.current; offset += terrainBatchSize) {
          try {
            const batch = work.requests.slice(offset, offset + terrainBatchSize);
            if (remote) {
              try {
                const uncached = batch.filter((request) => request.knownTimestamp === null);
                if (uncached.length) {
                  const cached = await displayCache.readTiles(profileId, work.dimension, work.resolution, uncached);
                  if (!terrainMountedRef.current || work.generation !== terrainGenerationRef.current) break;
                  for (const tile of cached) terrainCacheRef.current!.apply(work.dimension, tile);
                  if (cached.length) setTerrainEpoch((epoch) => epoch + 1);
                }
              } catch { /* A damaged or missing local cache must not prevent the remote read. */ }
            }
            const fresh = batch.map((request) => ({ ...request,
              knownTimestamp: work.force ? null : terrainCacheRef.current!.knownTimestamp(work.dimension, request.x, request.z, work.resolution) }));
            const results = await readTerrainTiles(remote, profileId, work.dimension, work.resolution, fresh);
            if (!terrainMountedRef.current) break;
            for (const result of results) {
              bytesRead += result.readBytes;
              if (result.status === "error") hadError = true;
              terrainCacheRef.current!.apply(work.dimension, result, work.force);
            }
            setTerrainEpoch((epoch) => epoch + 1);
            if (remote && results.some((tile) => tile.status === "tile")) {
              void displayCache.writeTiles(profileId, work.dimension, work.resolution, results).catch(() => {});
            }
            if (bytesRead >= terrainReadBudgetBytes) { limited = true; break; }
            await new Promise((resolve) => window.setTimeout(resolve, 75));
          } catch {
            hadError = true;
            break;
          }
        }
        if (terrainMountedRef.current && !terrainQueueRef.current && work.generation === terrainGenerationRef.current) {
          setTerrainWarning(limited ? "Terrain read limit reached. Zoom in to load more detail." : hadError ? "Some terrain could not be read. Cached tiles remain visible; try Refresh." : null);
        }
      }
    } finally {
      terrainBusyRef.current = false;
      if (terrainMountedRef.current && terrainQueueRef.current) void drainTerrain();
    }
  }

  useEffect(() => {
    if (!cacheReady || activeDimension === unknownDimension) {
      terrainQueueRef.current = null;
      terrainGenerationRef.current++;
      setTerrainWarning(null);
      return;
    }
    if (dragging) { terrainQueueRef.current = null; terrainGenerationRef.current++; return; }
    const timer = window.setTimeout(() => {
      if (!terrainMountedRef.current) return;
      const force = terrainRefresh !== lastTerrainRefreshRef.current;
      lastTerrainRefreshRef.current = terrainRefresh;
      const resolution = terrainResolution(currentView.unitsPerPixel);
      const now = Date.now();
      const requests = visibleChunks(currentView, width, height, terrainViewLimit, priorityChunks)
        .filter(([x, z]) => terrainCacheRef.current!.shouldRead(activeDimension, x, z, resolution, now, force))
        .map(([x, z]) => ({ x, z, knownTimestamp: force ? null : terrainCacheRef.current!.knownTimestamp(activeDimension, x, z, resolution) }));
      const generation = ++terrainGenerationRef.current;
      terrainQueueRef.current = requests.length ? { dimension: activeDimension, resolution, requests, generation, force } : null;
      if (requests.length) void drainTerrain();
    }, 250);
    return () => window.clearTimeout(timer);
  }, [activeDimension, currentView.centerX, currentView.centerZ, currentView.unitsPerPixel, width, height, dragging, terrainTick, terrainRefresh, priorityKey, profileId, remote, cacheReady]);

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
      <div className="player-map-title"><h2>Player map</h2><span className="player-map-progress">In Progress</span></div>
      <button className="button button-quiet" type="button" onClick={() => { setRevision((value) => value + 1); setTerrainRefresh((value) => value + 1); }} disabled={refreshing}><RefreshCw size={15} aria-hidden="true" /> {refreshing ? "Refreshing…" : "Refresh"}</button>
    </header>
    {warning && <p className="notice notice-error" role="status">{warning}</p>}
    {savedWarning && <p className="notice notice-error" role="status">{savedWarning}</p>}
    {terrainWarning && <p className="notice notice-error" role="status">{terrainWarning}</p>}
    <div className="player-map-dimensions" aria-label="Dimension">
      {(dimensions.length ? dimensions : ["minecraft:overworld"]).map((item) => <button key={item} type="button" className={item === activeDimension ? "player-map-dimension active" : "player-map-dimension"} onClick={() => { setDimension(item); setSelectedKey(null); }}>{dimensionLabel(item)} <span>{points.filter((point) => point.dimension === item).length}</span></button>)}
    </div>
    <div className="player-map-layout">
      <div ref={gridRef} className={`player-map-grid${dragging ? " player-map-grid-dragging" : ""}`} role="group" tabIndex={0}
        aria-label={`Player coordinate grid for ${dimensionLabel(activeDimension)}. Drag or use arrow keys to move; scroll or use the zoom buttons to zoom.`}
        onPointerDown={beginDrag} onPointerMove={moveDrag} onPointerUp={endDrag} onPointerCancel={endDrag} onKeyDown={moveWithKeyboard}>
        <canvas ref={terrainCanvasRef} className="player-map-terrain" aria-hidden="true" />
        {xLines.map((x) => <span key={`x-${x}`} className={`player-map-line player-map-line-x${x === 0 ? " player-map-line-origin" : ""}`} style={{ left: screenX(x) }}><span>{Math.round(x).toLocaleString()}</span></span>)}
        {zLines.map((z) => <span key={`z-${z}`} className={`player-map-line player-map-line-z${z === 0 ? " player-map-line-origin" : ""}`} style={{ top: screenY(z) }}><span>{Math.round(z).toLocaleString()}</span></span>)}
        <span className="player-map-north">North · −Z</span><span className="player-map-east">East · +X</span>
        <div className="player-map-controls" aria-label="Map controls">
          <button type="button" aria-label="Zoom in" title="Zoom in" onClick={() => zoom(0.8)}><Plus size={16} aria-hidden="true" /></button>
          <button type="button" aria-label="Zoom out" title="Zoom out" onClick={() => zoom(1.25)}><Minus size={16} aria-hidden="true" /></button>
          <button type="button" aria-label="Fit all players" title="Fit all players" onClick={() => setView(null)}><Maximize2 size={15} aria-hidden="true" /></button>
        </div>
        <span className="player-map-scale">{(step / 16).toLocaleString()} {(step / 16) === 1 ? "chunk" : "chunks"} / grid · {step.toLocaleString()} blocks</span>
        {visibleChunkCount > terrainViewLimit && <span className="player-map-coverage">Terrain loads near players and the map center. Zoom in for more.</span>}
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
