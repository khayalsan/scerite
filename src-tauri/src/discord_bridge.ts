import { execFile } from "node:child_process";
import { readFile, open, stat } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { Client, Events, GatewayIntentBits, type TextChannel } from "discord.js";

type Server = { id: string; name: string; directory: string; session: string; channel_id: string; chat: boolean; status: boolean; joins: boolean };
const root = dirname(fileURLToPath(import.meta.url));
const servers = JSON.parse(await readFile(join(root, "servers.json"), "utf8")) as Server[];
const token = (await readFile(join(root, "token"), "utf8")).trim();
const run = promisify(execFile);
const client = new Client({
  intents: [GatewayIntentBits.Guilds, GatewayIntentBits.GuildMessages, GatewayIntentBits.MessageContent],
  allowedMentions: { parse: [] },
});
const positions = new Map<string, { inode: number; offset: number }>();
const running = new Map<string, boolean>();
const ready = new Map<string, boolean>();

function clean(value: string, limit = 300) { return value.replace(/\s+/g, " ").trim().slice(0, limit); }

async function hasSession(name: string) {
  try {
    const { stdout } = await run("tmux", ["display-message", "-p", "-t", name, "#{pane_dead}"], { timeout: 5000 });
    return stdout.trim() === "0";
  }
  catch { return false; }
}

async function sendToMinecraft(server: Server, author: string, content: string) {
  if (!await hasSession(server.session)) return;
  const command = `say ${clean(`[Discord] ${author}: ${content}`, 400)}`;
  await run("tmux", ["send-keys", "-t", server.session, "-l", command], { timeout: 5000 });
  await run("tmux", ["send-keys", "-t", server.session, "Enter"], { timeout: 5000 });
}

async function announce(server: Server, text: string) {
  try {
    const channel = await client.channels.fetch(server.channel_id);
    if (!channel?.isTextBased() || !("send" in channel)) return;
    await (channel as TextChannel).send({ content: text, allowedMentions: { parse: [] } });
  } catch (error) { console.error(`Could not post to ${server.name}'s Discord channel: ${String(error)}`); }
}

async function newLines(server: Server) {
  const path = join(server.directory, "logs", "latest.log");
  let info;
  try { info = await stat(path); } catch { return []; }
  const previous = positions.get(server.id);
  if (!previous && ready.get(server.id)) { positions.set(server.id, { inode: info.ino, offset: info.size }); return []; }
  const offset = !previous || previous.inode !== info.ino || previous.offset > info.size ? 0 : previous.offset;
  const size = Math.min(info.size - offset, 64 * 1024);
  if (!size) return [];
  const file = await open(path, "r");
  try {
    const bytes = Buffer.alloc(size);
    const { bytesRead } = await file.read(bytes, 0, size, offset);
    const complete = bytes.lastIndexOf(10, bytesRead - 1);
    if (complete < 0) return [];
    positions.set(server.id, { inode: info.ino, offset: offset + complete + 1 });
    return bytes.subarray(0, complete + 1).toString("utf8").split(/\r?\n/).filter(Boolean);
  } finally { await file.close(); }
}

async function watchServers() {
  while (true) {
    for (const server of servers) {
      try {
        const active = await hasSession(server.session);
        const previous = running.get(server.id);
        running.set(server.id, active);
        if (previous === undefined) ready.set(server.id, active);
        else if (previous && !active) {
          if (server.status) await announce(server, `🔴 **${server.name}** is offline.`);
          ready.set(server.id, false);
        } else if (!previous && active) ready.set(server.id, false);
        for (const line of await newLines(server)) {
          if (active && !ready.get(server.id) && line.includes("Done (")) {
            ready.set(server.id, true);
            if (server.status) await announce(server, `🟢 **${server.name}** is online.`);
          }
          const chat = line.match(/\]: <([^<>]{1,32})> (.+)$/);
          if (server.chat && chat) { await announce(server, `**${clean(chat[1], 32)}:** ${clean(chat[2])}`); continue; }
          if (!server.joins) continue;
          const joined = line.match(/\]: ([A-Za-z0-9_]{1,16}) joined the game$/);
          if (joined) await announce(server, `${joined[1]} joined **${server.name}**.`);
          const left = line.match(/\]: ([A-Za-z0-9_]{1,16}) left the game$/);
          if (left) await announce(server, `${left[1]} left **${server.name}**.`);
        }
      } catch (error) { console.error(`Could not read ${server.name}: ${String(error)}`); }
    }
    await new Promise((resolve) => setTimeout(resolve, 2000));
  }
}

client.on(Events.MessageCreate, (message) => {
  if (!message.guild || message.author.bot || message.webhookId) return;
  const server = servers.find((item) => item.chat && item.channel_id === message.channel.id);
  if (!server || !message.content.trim()) return;
  void sendToMinecraft(server, clean(message.author.displayName, 40), clean(message.content))
    .catch((error) => console.error(`Could not deliver Discord chat to ${server.name}: ${String(error)}`));
});
client.once(Events.ClientReady, () => { void watchServers(); });
client.on(Events.Error, (error) => console.error(`Discord connection error: ${String(error)}`));
try { await client.login(token); }
catch { console.error("Discord login failed. Check the bot token and Message Content intent."); process.exitCode = 1; }
