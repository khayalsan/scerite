import { invoke } from "@tauri-apps/api/core";
import type { TerrainRequest, TerrainResult, TileResolution } from "./terrainTiles";
import type { PlayerRecord } from "./serverPlayers";
import type { ServerLogSnapshot } from "./serverLogs";

export type CachedMapSnapshot = {
  players: PlayerRecord[];
  saved: Record<string, { position: [number, number, number]; dimension: string | null }>;
  view: { dimension: string; centerX: number; centerZ: number; unitsPerPixel: number } | null;
};
export type CachedLogs = { names: string[]; latest: ServerLogSnapshot | null };

export const displayCache = {
  readTiles: (profileId: string, dimension: string, resolution: TileResolution, requests: TerrainRequest[]) =>
    invoke<TerrainResult[]>("read_cached_terrain_tiles", { profileId, dimension, resolution, requests }),
  writeTiles: (profileId: string, dimension: string, resolution: TileResolution, tiles: TerrainResult[]) =>
    invoke<void>("write_cached_terrain_tiles", { profileId, dimension, resolution,
      tiles: tiles.filter((tile) => tile.status === "tile" && tile.rgba).map(({ x, z, timestamp, rgba }) => ({ x, z, timestamp, rgba })) }),
  readMap: (profileId: string) => invoke<CachedMapSnapshot | null>("read_cached_map_snapshot", { profileId }),
  writeMap: (profileId: string, snapshot: CachedMapSnapshot) => invoke<void>("write_cached_map_snapshot", { profileId, snapshot }),
  readLogs: (profileId: string) => invoke<CachedLogs | null>("read_cached_logs", { profileId }),
  writeLogs: (profileId: string, snapshot: CachedLogs) => invoke<void>("write_cached_logs", { profileId, snapshot }),
  readIcon: (profileId: string) => invoke<number[] | null>("read_cached_server_icon", { profileId }),
  writeIcon: (profileId: string, bytes: number[] | null) => invoke<void>("write_cached_server_icon", { profileId, bytes }),
  size: () => invoke<number>("display_cache_size"),
  clear: () => invoke<void>("clear_display_cache"),
};
