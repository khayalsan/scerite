import { execFile } from "node:child_process";
import { readFile, open, stat } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { Client, EmbedBuilder, Events, GatewayIntentBits, TextChannel, type Webhook } from "discord.js";

type Action = "chat_to_discord" | "chat_to_minecraft" | "online" | "offline" | "join" | "leave" | "kills" | "achievements";
type Server = { id: string; name: string; directory: string; session: string; channel_id: string; chat_to_minecraft_template?: string } & Record<Action, boolean> & Record<`${Action}_channel_id`, string>;
const defaultMinecraftTemplate = "<🎮{username}> {message}";
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
const knownPlayers = new Map<string, Set<string>>();
const playerIds = new Map<string, Map<string, string>>();
const webhooks = new Map<string, Promise<Webhook>>();

for (const server of servers) {
  try {
    const entries = JSON.parse(await readFile(join(server.directory, "usercache.json"), "utf8")) as { name?: string; uuid?: string }[];
    knownPlayers.set(server.id, new Set(entries.map((entry) => entry.name).filter((name): name is string => !!name)));
    playerIds.set(server.id, new Map(entries.filter((entry) => entry.name && entry.uuid).map((entry) => [entry.name!.toLowerCase(), entry.uuid!])));
  } catch { knownPlayers.set(server.id, new Set()); playerIds.set(server.id, new Map()); }
}

function clean(value: string, limit = 300) { return value.replace(/\s+/g, " ").trim().slice(0, limit); }

async function hasSession(name: string) {
  try {
    const { stdout } = await run("tmux", ["display-message", "-p", "-t", name, "#{pane_dead}"], { timeout: 5000 });
    return stdout.trim() === "0";
  }
  catch { return false; }
}

async function sendToMinecraft(server: Server, author: string, content: string) {
  let available = false;
  for (let attempt = 0; attempt < 3; attempt++) {
    if (await hasSession(server.session)) { available = true; break; }
    if (attempt < 2) await new Promise((resolve) => setTimeout(resolve, 500));
  }
  if (!available) throw new Error("Minecraft console was unavailable after three checks");
  const template = server.chat_to_minecraft_template ?? defaultMinecraftTemplate;
  const message = clean(template.replace(/\{username\}|\{message\}/g, (placeholder) => placeholder === "{username}" ? author : content), 400)
    .replace(/[\u0000-\u001f\u007f]/g, " ");
  const command = `tellraw @a ${JSON.stringify({ text: message })}`;
  await run("tmux", ["send-keys", "-t", server.session, command, "Enter"], { timeout: 5000 });
}

function channelFor(server: Server, action: Action) {
  return server[`${action}_channel_id`] || server.channel_id;
}

async function announce(server: Server, action: Action, text: string) {
  const channelId = channelFor(server, action);
  if (!server[action] || !channelId) return;
  try {
    const channel = await client.channels.fetch(channelId);
    if (!channel?.isTextBased() || !("send" in channel)) return;
    await (channel as TextChannel).send({ content: text, allowedMentions: { parse: [] } });
  } catch (error) { console.error(`Could not post to ${server.name}'s Discord channel: ${String(error)}`); }
}

async function announceEmbed(server: Server, action: Action, title: string, description: string, color: number) {
  const channelId = channelFor(server, action);
  if (!server[action] || !channelId) return;
  try {
    const channel = await client.channels.fetch(channelId);
    if (!channel?.isTextBased() || !("send" in channel)) return;
    const embed = new EmbedBuilder().setTitle(title).setDescription(description).setColor(color);
    await (channel as TextChannel).send({ embeds: [embed], allowedMentions: { parse: [] } });
  } catch (error) { console.error(`Could not post to ${server.name}'s Discord channel: ${String(error)}`); }
}

async function webhookFor(channel: TextChannel) {
  let pending = webhooks.get(channel.id);
  if (!pending) {
    pending = (async () => {
      const existing = await channel.fetchWebhooks();
      const hook = existing.find((item) => item.name === "Scerite" && item.owner?.id === client.user?.id && item.token);
      return hook ?? channel.createWebhook({ name: "Scerite", reason: "Minecraft chat bridge" });
    })();
    webhooks.set(channel.id, pending);
    void pending.catch(() => webhooks.delete(channel.id));
  }
  return pending;
}

async function announceChat(server: Server, player: string, message: string) {
  const channelId = channelFor(server, "chat_to_discord");
  if (!server.chat_to_discord || !channelId) return;
  try {
    const channel = await client.channels.fetch(channelId);
    if (!channel?.isTextBased() || !("send" in channel)) return;
    if (!(channel instanceof TextChannel)) {
      await channel.send({ content: `**${player}:** ${message}`, allowedMentions: { parse: [] } });
      return;
    }
    let hook: Webhook;
    try { hook = await webhookFor(channel); }
    catch (error) {
      console.error(`Could not prepare a Discord webhook for ${server.name}; grant Manage Webhooks: ${String(error)}`);
      await channel.send({ content: `**${player}:** ${message}`, allowedMentions: { parse: [] } });
      return;
    }
    const identity = playerIds.get(server.id)?.get(player.toLowerCase()) ?? player;
    await hook.send({ content: message, username: player, avatarURL: `https://mc-heads.net/avatar/${encodeURIComponent(identity)}/64`, allowedMentions: { parse: [] } });
  } catch (error) { console.error(`Could not post Minecraft chat from ${player} to Discord: ${String(error)}`); }
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
  const outbound: Action[] = ["chat_to_discord", "online", "offline", "join", "leave", "kills", "achievements"];
  const watchedServers = servers.filter((server) => outbound.some((action) => server[action] && channelFor(server, action)));
  while (true) {
    for (const server of watchedServers) {
      try {
        const active = await hasSession(server.session);
        const previous = running.get(server.id);
        running.set(server.id, active);
        if (previous === undefined) ready.set(server.id, active);
        else if (previous && !active) {
          await announce(server, "offline", `🔴 **${server.name}** is offline.`);
          ready.set(server.id, false);
        } else if (!previous && active) ready.set(server.id, false);
        for (const line of await newLines(server)) {
          if (active && !ready.get(server.id) && line.includes("Done (")) {
            ready.set(server.id, true);
            await announce(server, "online", `🟢 **${server.name}** is online.`);
          }
          const chat = line.match(/\]: <([^<>]{1,32})> (.+)$/);
          if (chat) { await announceChat(server, clean(chat[1], 32), clean(chat[2])); continue; }
          const joined = line.match(/\]: ([A-Za-z0-9_]{1,16}) joined the game$/);
          if (joined) { knownPlayers.get(server.id)?.add(joined[1]); await announceEmbed(server, "join", "Player joined", `**${joined[1]}** joined **${server.name}**.`, 0x57b87b); continue; }
          const left = line.match(/\]: ([A-Za-z0-9_]{1,16}) left the game$/);
          if (left) { await announceEmbed(server, "leave", "Player left", `**${left[1]}** left **${server.name}**.`, 0xe2aa55); continue; }
          const achievement = line.match(/\]: ([A-Za-z0-9_]{1,16}) has (?:made the advancement|completed the challenge|reached the goal) \[([^\]]{1,100})\]$/);
          if (achievement) { await announceEmbed(server, "achievements", "Advancement earned", `**${achievement[1]}** earned **${clean(achievement[2], 100)}**.`, 0xd5aa38); continue; }
          const death = line.match(/\]: ([A-Za-z0-9_]{1,16}) ((?:was |drowned|died|fell |hit |burned |went |tried |starved|suffocated|froze|walked |experienced |discovered |blew ).*)$/);
          if (death && knownPlayers.get(server.id)?.has(death[1])) await announceEmbed(server, "kills", "Player death", `**${death[1]}** ${clean(death[2])}`, 0xd06363);
        }
      } catch (error) { console.error(`Could not read ${server.name}: ${String(error)}`); }
    }
    await new Promise((resolve) => setTimeout(resolve, 2000));
  }
}

client.on(Events.MessageCreate, (message) => {
  if (!message.guild || message.author.bot || message.webhookId) return;
  if (!message.content.trim()) return;
  for (const server of servers.filter((item) => item.chat_to_minecraft && channelFor(item, "chat_to_minecraft") === message.channel.id)) {
    void sendToMinecraft(server, clean(message.author.displayName, 40), clean(message.content))
      .catch((error) => console.error(`Could not deliver Discord message ${message.id} to ${server.name}: ${String(error)}`));
  }
});
client.once(Events.ClientReady, () => { void watchServers(); });
client.on(Events.Error, (error) => console.error(`Discord connection error: ${String(error)}`));
try { await client.login(token); }
catch { console.error("Discord login failed. Check the bot token and Message Content intent."); process.exitCode = 1; }
