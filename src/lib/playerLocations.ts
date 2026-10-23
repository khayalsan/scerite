import { invoke } from "@tauri-apps/api/core";
import { appendedLines } from "./onlinePlayers";
import { serverSessions } from "./serverSessions";

export interface LivePlayerLocation {
  name: string;
  position: [number, number, number];
  dimension: string | null;
}

export interface SavedPlayerLocation {
  uuid: string;
  position: [number, number, number] | null;
  dimension: string | null;
}

export const readSavedPlayerLocations = (remote: boolean, profileId: string, uuids: string[]) =>
  invoke<SavedPlayerLocation[]>(remote ? "read_remote_player_locations" : "read_local_player_locations", { profileId, uuids });

const number = "[+-]?(?:\\d+(?:\\.\\d*)?|\\.\\d+)(?:[Ee][+-]?\\d+)?";
const positionPattern = new RegExp(`^\\[\\s*(${number})[dDfF]?,\\s*(${number})[dDfF]?,\\s*(${number})[dDfF]?\\s*\\]`);

function parseResponse(line: string): { name: string; value: string } | null {
  const clean = line.replace(/\x1b\[[0-9;?]*[A-Za-z]/g, "").replace(/§[0-9a-fk-or]/gi, "");
  const match = /\b([A-Za-z0-9_]{3,16}) has (?:the )?following entity data:\s*(.+)$/i.exec(clean);
  return match ? { name: match[1], value: match[2].trim() } : null;
}

export async function readLivePlayerLocations(remote: boolean, profileId: string, expectedNames: string[]): Promise<LivePlayerLocation[]> {
  let remoteBefore = remote ? (await serverSessions.remoteConsole(profileId)).lines : [];
  let localCursor = remote ? 0 : (await serverSessions.console(profileId, 0)).nextSequence - 1;
  const send = remote ? serverSessions.sendRemoteInput : serverSessions.sendInput;
  await send(profileId, "execute as @a run data get entity @s Pos");
  await send(profileId, "execute as @a run data get entity @s Dimension");

  const positions = new Map<string, { name: string; position: [number, number, number]; dimension: string | null }>();
  const dimensions = new Map<string, string>();
  const expected = new Set(expectedNames.map((name) => name.toLowerCase()));
  const deadline = Date.now() + 6000;
  while (Date.now() < deadline) {
    await new Promise((resolve) => window.setTimeout(resolve, 500));
    let lines: string[];
    if (remote) {
      const next = (await serverSessions.remoteConsole(profileId)).lines;
      lines = appendedLines(remoteBefore, next);
      remoteBefore = next;
    } else {
      const next = await serverSessions.console(profileId, localCursor);
      lines = next.lines.map((line) => line.text);
      localCursor = next.nextSequence - 1;
    }
    for (const line of lines) {
      const response = parseResponse(line);
      if (!response) continue;
      const key = response.name.toLowerCase();
      const pos = positionPattern.exec(response.value);
      if (pos) {
        const coordinates = [Number(pos[1]), Number(pos[2]), Number(pos[3])];
        if (coordinates.every(Number.isFinite)) positions.set(key, { name: response.name, position: coordinates as [number, number, number], dimension: null });
      } else {
        const dimension = /^"?([a-z0-9_.-]+:[a-z0-9_./-]+)"?$/i.exec(response.value)?.[1];
        if (dimension) dimensions.set(key, dimension);
      }
    }
    if (expected.size && [...expected].every((name) => positions.has(name) && dimensions.has(name))) break;
  }
  return [...positions.entries()].map(([key, value]) => ({ ...value, dimension: dimensions.get(key) ?? null }));
}
