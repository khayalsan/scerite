import { invoke } from "@tauri-apps/api/core";
import { serverSessions } from "./serverSessions";

export interface OnlinePlayerCache {
  names: string[];
  response: string;
  updatedAt: number;
}

const playerName = /^[A-Za-z0-9_]{3,16}$/;

function parseListResponse(lines: string[]): { names: string[]; response: string } | null {
  for (let index = lines.length - 1; index >= 0; index--) {
    for (let length = 1; length <= 3 && index + length <= lines.length; length++) {
      const clean = lines.slice(index, index + length).join(" ")
        .replace(/\x1b\[[0-9;?]*[A-Za-z]/g, "").replace(/§[0-9a-fk-or]/gi, "");
      const match = /\bThere (?:are|is) (\d+)(?:\/\d+| (?:of a max of|out of maximum|of) \d+) players? online[:.]?\s*(.*)$/i.exec(clean);
      if (!match) continue;
      const count = Number(match[1]);
      const names = match[2].replace(/^[:.\s]+/, "").split(",").map((name) => name.trim()).filter(Boolean);
      if (count === names.length && names.every((name) => playerName.test(name))) {
        return { names, response: match[0] };
      }
    }
  }
  return null;
}

function appendedLines(before: string[], after: string[]): string[] {
  if (before.length === after.length && before.every((line, index) => line === after[index])) return [];
  for (let trim = 0; trim <= 2; trim++) {
    const stable = before.slice(0, before.length - trim);
    for (let start = 0; start < stable.length; start++) {
      const overlap = stable.length - start;
      if (overlap <= after.length && stable.slice(start).every((line, index) => line === after[index])) {
        return after.slice(overlap);
      }
    }
  }
  return before.length ? [] : after;
}

export const onlinePlayers = {
  cached: (profileId: string) => invoke<OnlinePlayerCache | null>("read_online_player_cache", { profileId }),
  refresh: async (remote: boolean, profileId: string): Promise<{ names: string[]; cacheWarning: string | null }> => {
    const previous = remote ? (await serverSessions.remoteConsole(profileId)).lines : [];
    let cursor = remote ? 0 : (await serverSessions.console(profileId, 0)).nextSequence - 1;
    if (remote) await serverSessions.sendRemoteInput(profileId, "list");
    else await serverSessions.sendInput(profileId, "list");

    const deadline = Date.now() + 10_000;
    while (Date.now() < deadline) {
      await new Promise((resolve) => setTimeout(resolve, 500));
      let lines: string[];
      if (remote) lines = appendedLines(previous, (await serverSessions.remoteConsole(profileId)).lines);
      else {
        const snapshot = await serverSessions.console(profileId, cursor);
        cursor = snapshot.nextSequence - 1;
        lines = snapshot.lines.map((line) => line.text);
      }
      const result = parseListResponse(lines);
      if (result) {
        try {
          await invoke<OnlinePlayerCache>("write_online_player_cache", { profileId, ...result });
          return { names: result.names, cacheWarning: null };
        } catch {
          return { names: result.names, cacheWarning: "Online players were found, but their cache could not be saved." };
        }
      }
    }
    throw new Error("The server did not return a readable response to the list command.");
  },
};
