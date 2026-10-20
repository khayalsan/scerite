import { invoke } from "@tauri-apps/api/core";
import { serverLogs } from "./serverLogs";

export interface PlayerStatistic { category: string; key: string; value: number }
export interface PlayerData {
  hasSavedData: boolean;
  hasSavedStatistics: boolean;
  gamemode: string | null;
  level: number | null;
  health: number | null;
  hunger: number | null;
  position: [number, number, number] | null;
  statistics: PlayerStatistic[];
  dataWarning: string | null;
  statsWarning: string | null;
}

export interface PlayerRecord {
  key: string;
  uuid: string | null;
  name: string | null;
  online: boolean;
  whitelisted: boolean;
  operator: boolean;
  banned: boolean;
}

export interface AccessEntry { value: string; reason: string | null }
export interface AccessLists {
  whitelist: AccessEntry[];
  operators: AccessEntry[];
  bannedNames: AccessEntry[];
  bannedIps: AccessEntry[];
}

const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const playerNamePattern = /^[A-Za-z0-9_]{3,16}$/;

type NamedEntry = { uuid?: unknown; name?: unknown; ip?: unknown; reason?: unknown };
type PlayerRosterFiles = { cache: string; whitelist: string; ops: string; bans: string; bannedIps: string; uuids: string[] };

function namedEntries(value: string): NamedEntry[] {
  try {
    const parsed: unknown = JSON.parse(value);
    return Array.isArray(parsed) ? parsed.filter((entry): entry is NamedEntry => typeof entry === "object" && entry !== null) : [];
  } catch { return []; }
}

function onlineNames(log: string): Set<string> {
  const names = new Set<string>();
  for (const line of log.split(/\r?\n/)) {
    const event = line.match(/(?:\]: |\]: \[[^\]]+\]: )([A-Za-z0-9_]{3,16}) (joined|left) the game\b/);
    if (!event) continue;
    if (event[2] === "joined") names.add(event[1].toLowerCase());
    else names.delete(event[1].toLowerCase());
  }
  return names;
}

export async function loadPlayers(remote: boolean, profileId: string, running: boolean): Promise<{ players: PlayerRecord[]; lists: AccessLists; warning: string | null }> {
  const { cache, whitelist, ops, bans, bannedIps, uuids } = await invoke<PlayerRosterFiles>(
    remote ? "read_remote_player_roster" : "read_local_player_roster", { profileId },
  );
  const listNames = (value: string): AccessEntry[] => namedEntries(value).map((entry) => ({
    value: typeof entry.name === "string" && entry.name ? entry.name : typeof entry.uuid === "string" ? entry.uuid : "",
    reason: typeof entry.reason === "string" ? entry.reason : null,
  })).filter((entry) => entry.value !== "");
  const lists: AccessLists = {
    whitelist: listNames(whitelist), operators: listNames(ops), bannedNames: listNames(bans),
    bannedIps: namedEntries(bannedIps).map((entry) => ({
      value: typeof entry.ip === "string" ? entry.ip : "",
      reason: typeof entry.reason === "string" ? entry.reason : null,
    })).filter((entry) => entry.value !== ""),
  };
  const byKey = new Map<string, PlayerRecord>();
  const ensure = (uuidValue: unknown, nameValue: unknown): PlayerRecord | null => {
    const uuid = typeof uuidValue === "string" && uuidPattern.test(uuidValue) ? uuidValue.toLowerCase() : null;
    const name = typeof nameValue === "string" && playerNamePattern.test(nameValue) ? nameValue : null;
    if (!uuid && !name) return null;
    const key = uuid ?? name!.toLowerCase();
    const old = byKey.get(key) ?? (name ? [...byKey.values()].find((item) => item.name?.toLowerCase() === name.toLowerCase()) : undefined);
    const player = old ?? { key, uuid, name, online: false, whitelisted: false, operator: false, banned: false };
    player.uuid = player.uuid ?? uuid;
    player.name = player.name ?? name;
    player.key = player.uuid ?? player.name!.toLowerCase();
    if (old) byKey.delete(old.key);
    if (name) byKey.delete(name.toLowerCase());
    byKey.set(player.key, player);
    return player;
  };
  for (const entry of namedEntries(cache)) ensure(entry.uuid, entry.name);
  for (const entry of namedEntries(whitelist)) { const player = ensure(entry.uuid, entry.name); if (player) player.whitelisted = true; }
  for (const entry of namedEntries(ops)) { const player = ensure(entry.uuid, entry.name); if (player) player.operator = true; }
  for (const entry of namedEntries(bans)) { const player = ensure(entry.uuid, entry.name); if (player) player.banned = true; }

  for (const uuid of uuids) ensure(uuid, null);

  let warning: string | null = null;
  if (running) {
    try {
      const log = await serverLogs.read(remote, profileId, "latest.log");
      const seen = onlineNames(log.text);
      for (const name of seen) { const player = ensure(null, name); if (player) player.online = true; }
      if (log.truncated) warning = "Only the recent part of latest.log was read. Earlier joins may be missed.";
    } catch { warning = "The latest log could not be read, so online status is unavailable."; }
  }
  return { players: [...byKey.values()].sort((a, b) => Number(b.online) - Number(a.online) || (a.name ?? a.uuid ?? "").localeCompare(b.name ?? b.uuid ?? "")), lists, warning };
}

export const playerData = (remote: boolean, profileId: string, uuid: string) =>
  invoke<PlayerData>(remote ? "read_remote_player_data" : "read_local_player_data", { profileId, uuid });

export const validPlayerName = (name: string) => playerNamePattern.test(name);

export const validIpAddress = (value: string) => {
  if (/^(?:\d{1,3}\.){3}\d{1,3}$/.test(value)) return value.split(".").every((part) => Number(part) <= 255);
  if (!value.includes(":") || !/^[0-9a-fA-F:]+$/.test(value) || value.length > 39) return false;
  try { return new URL(`http://[${value}]/`).hostname.length > 0; }
  catch { return false; }
};
