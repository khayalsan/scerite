import { invoke } from "@tauri-apps/api/core";

export type TileResolution = 1 | 4 | 16;
export type TerrainRequest = { x: number; z: number; knownTimestamp: number | null };
export type TerrainResult = {
  x: number;
  z: number;
  timestamp: number | null;
  status: "tile" | "unchanged" | "missing" | "error";
  resolution: TileResolution;
  rgba: number[] | null;
  readBytes: number;
};

type CachedTile = {
  timestamp: number | null;
  resolution: TileResolution;
  canvas: HTMLCanvasElement | null;
  checkedAt: number;
  verified: boolean;
};

const key = (dimension: string, x: number, z: number, resolution: TileResolution) => `${dimension}|${x}|${z}|${resolution}`;

function smaller(resolution: TileResolution): TileResolution[] {
  return resolution === 16 ? [4, 1] : resolution === 4 ? [1] : [];
}

export function terrainResolution(unitsPerPixel: number): TileResolution {
  return unitsPerPixel <= 0.4 ? 16 : unitsPerPixel <= 1.6 ? 4 : 1;
}

export const readTerrainTiles = (remote: boolean, profileId: string, dimension: string, resolution: TileResolution, requests: TerrainRequest[]) =>
  invoke<TerrainResult[]>(remote ? "read_remote_terrain_tiles" : "read_local_terrain_tiles", { profileId, dimension, resolution, requests });

export class TerrainTileCache {
  private entries = new Map<TileResolution, Map<string, CachedTile>>([
    [1, new Map()], [4, new Map()], [16, new Map()],
  ]);
  private errors = new Map<string, number>();

  constructor(private readonly limitPerResolution = 512) {}

  private touch(tileKey: string, tile: CachedTile) {
    const entries = this.entries.get(tile.resolution)!;
    entries.delete(tileKey);
    entries.set(tileKey, tile);
    while (entries.size > this.limitPerResolution) entries.delete(entries.keys().next().value!);
  }

  private exact(dimension: string, x: number, z: number, resolution: TileResolution) {
    return this.entries.get(resolution)!.get(key(dimension, x, z, resolution));
  }

  private derive(dimension: string, x: number, z: number, resolution: TileResolution) {
    const source = ([16, 4] as TileResolution[]).find((candidate) => candidate > resolution && this.exact(dimension, x, z, candidate)?.canvas);
    if (!source) return null;
    const original = this.exact(dimension, x, z, source)!;
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = resolution;
    const context = canvas.getContext("2d");
    if (!context) return null;
    context.imageSmoothingEnabled = true;
    context.drawImage(original.canvas!, 0, 0, resolution, resolution);
    const derived = { timestamp: original.timestamp, resolution, canvas, checkedAt: original.checkedAt, verified: original.verified };
    this.touch(key(dimension, x, z, resolution), derived);
    return derived;
  }

  tile(dimension: string, x: number, z: number, resolution: TileResolution): CachedTile | null {
    const exact = this.exact(dimension, x, z, resolution) ?? this.derive(dimension, x, z, resolution);
    if (exact) {
      this.touch(key(dimension, x, z, resolution), exact);
      return exact;
    }
    for (const fallback of smaller(resolution)) {
      const tile = this.exact(dimension, x, z, fallback);
      if (tile?.canvas) return tile;
    }
    return null;
  }

  coordinates(dimension: string) {
    const found = new Map<string, [number, number]>();
    for (const entries of this.entries.values()) {
      for (const tileKey of entries.keys()) {
        const parts = tileKey.split("|");
        if (parts[0] === dimension) found.set(`${parts[1]}|${parts[2]}`, [Number(parts[1]), Number(parts[2])]);
      }
    }
    return found.values();
  }

  shouldRead(dimension: string, x: number, z: number, resolution: TileResolution, now: number, force: boolean) {
    const tileKey = key(dimension, x, z, resolution);
    if (!force && now - (this.errors.get(tileKey) ?? 0) < 15_000) return false;
    const tile = this.exact(dimension, x, z, resolution) ?? this.derive(dimension, x, z, resolution);
    return force || !tile || !tile.verified || now - tile.checkedAt >= 45_000;
  }

  knownTimestamp(dimension: string, x: number, z: number, resolution: TileResolution) {
    const tile = this.exact(dimension, x, z, resolution);
    return tile?.verified ? tile.timestamp : null;
  }

  apply(dimension: string, result: TerrainResult, source: "remote" | "disk" = "remote") {
    const tileKey = key(dimension, result.x, result.z, result.resolution);
    const now = Date.now();
    if (result.status === "error") {
      this.errors.set(tileKey, now);
      while (this.errors.size > this.limitPerResolution) this.errors.delete(this.errors.keys().next().value!);
      return false;
    }
    this.errors.delete(tileKey);
    if (result.status === "unchanged") {
      const tile = this.entries.get(result.resolution)!.get(tileKey);
      if (tile) this.touch(tileKey, { ...tile, checkedAt: now });
      return false;
    }
    if (result.status === "missing") {
      const cached = ([1, 4, 16] as TileResolution[])
        .map((resolution) => [key(dimension, result.x, result.z, resolution), this.exact(dimension, result.x, result.z, resolution)] as const)
        .filter(([, tile]) => tile?.canvas);
      if (cached.length) {
        for (const [cachedKey, tile] of cached) this.touch(cachedKey, { ...tile!, checkedAt: now });
        return false;
      }
      this.touch(tileKey, { timestamp: null, resolution: result.resolution, canvas: null, checkedAt: now, verified: true });
      return true;
    }
    if (!result.rgba || result.rgba.length !== result.resolution * result.resolution * 4) {
      this.errors.set(tileKey, now);
      return false;
    }
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = result.resolution;
    const context = canvas.getContext("2d");
    if (!context) { this.errors.set(tileKey, now); return false; }
    context.putImageData(new ImageData(new Uint8ClampedArray(result.rgba), result.resolution, result.resolution), 0, 0);
    for (const resolution of [1, 4, 16] as TileResolution[]) {
      const existing = this.exact(dimension, result.x, result.z, resolution);
      if (resolution !== result.resolution && existing && existing.timestamp !== result.timestamp) {
        this.touch(key(dimension, result.x, result.z, resolution), { ...existing, checkedAt: 0 });
      }
    }
    this.touch(tileKey, { timestamp: result.timestamp, resolution: result.resolution, canvas, checkedAt: now, verified: source === "remote" });
    return true;
  }
}
