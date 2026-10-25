import { execFile } from "node:child_process";
import { readFile, open, stat, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { inflateRawSync } from "node:zlib";
import { Client, EmbedBuilder, Events, GatewayIntentBits, TextChannel, type Webhook } from "discord.js";

type Action = "chat_to_discord" | "chat_to_minecraft" | "online" | "offline" | "join" | "leave" | "kills" | "achievements";
type Server = { id: string; name: string; directory: string; session: string; channel_id: string; minecraft_version?: string | null; chat_to_minecraft_template?: string } & Record<Action, boolean> & Record<`${Action}_channel_id`, string>;
type AttachmentLink = { label: string; url: string };
const defaultMinecraftTemplate = "<{color:blue}🎮{color:reset}{name}> {message}";
const legacyMinecraftTemplates = new Set(["<🎮{username}> {message}", "<🎮{color:blue}{name}{color:reset}> {message}", "<{color:blue}🎮{name}{color:reset}> {message}"]);
const minecraftColors = new Set(["black", "dark_blue", "dark_green", "dark_aqua", "dark_red", "dark_purple", "gold", "gray", "dark_gray", "blue", "green", "aqua", "red", "light_purple", "yellow", "white"]);
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
const advancementDescriptions = new Map<string, Promise<Map<string, string>>>();
const serverVersions = new Map<string, Promise<string | null>>();

for (const server of servers) {
  try {
    const entries = JSON.parse(await readFile(join(server.directory, "usercache.json"), "utf8")) as { name?: string; uuid?: string }[];
    knownPlayers.set(server.id, new Set(entries.map((entry) => entry.name).filter((name): name is string => !!name)));
    playerIds.set(server.id, new Map(entries.filter((entry) => entry.name && entry.uuid).map((entry) => [entry.name!.toLowerCase(), entry.uuid!])));
  } catch { knownPlayers.set(server.id, new Set()); playerIds.set(server.id, new Map()); }
}

function clean(value: string, limit = 300) { return value.replace(/\s+/g, " ").trim().slice(0, limit); }

async function mojangBytes(url: string, limit: number) {
  const target = new URL(url);
  if (target.protocol !== "https:" || !target.hostname.endsWith(".mojang.com")) throw new Error("Unexpected Minecraft metadata URL");
  const response = await fetch(target, { signal: AbortSignal.timeout(60000) });
  if (!response.ok || !response.body) throw new Error(`Minecraft metadata returned ${response.status}`);
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.length;
    if (size > limit) { await reader.cancel(); throw new Error("Minecraft metadata is too large"); }
    chunks.push(value);
  }
  return Buffer.concat(chunks);
}

function zipEntry(archive: Buffer, name: string) {
  let end = -1;
  for (let offset = archive.length - 22; offset >= Math.max(0, archive.length - 65557); offset--) {
    if (archive.readUInt32LE(offset) === 0x06054b50 && offset + 22 + archive.readUInt16LE(offset + 20) === archive.length) { end = offset; break; }
  }
  if (end < 0) throw new Error("Minecraft client archive is invalid");
  const directory = archive.readUInt32LE(end + 16);
  const directoryEnd = directory + archive.readUInt32LE(end + 12);
  if (directoryEnd > end) throw new Error("Minecraft client archive directory is invalid");
  for (let offset = directory; offset < directoryEnd;) {
    if (offset + 46 > directoryEnd || archive.readUInt32LE(offset) !== 0x02014b50) break;
    const filenameLength = archive.readUInt16LE(offset + 28);
    const next = offset + 46 + filenameLength + archive.readUInt16LE(offset + 30) + archive.readUInt16LE(offset + 32);
    if (next > directoryEnd) break;
    if (archive.toString("utf8", offset + 46, offset + 46 + filenameLength) === name) {
      const compressedSize = archive.readUInt32LE(offset + 20);
      const originalSize = archive.readUInt32LE(offset + 24);
      if (originalSize > 2 * 1024 * 1024) throw new Error("Minecraft language file is too large");
      const local = archive.readUInt32LE(offset + 42);
      if (local + 30 > archive.length || archive.readUInt32LE(local) !== 0x04034b50) break;
      const start = local + 30 + archive.readUInt16LE(local + 26) + archive.readUInt16LE(local + 28);
      if (start + compressedSize > archive.length) break;
      const data = archive.subarray(start, start + compressedSize);
      const method = archive.readUInt16LE(offset + 10);
      const plain = method === 8 ? inflateRawSync(data, { maxOutputLength: 2 * 1024 * 1024 }) : method === 0 ? data : null;
      if (!plain || plain.length !== originalSize) throw new Error("Minecraft language file is invalid");
      return plain.toString("utf8");
    }
    offset = next;
  }
  throw new Error("Minecraft English language file was not found");
}

async function loadAdvancementDescriptions(version: string) {
  const cache = join(root, `advancements-${version}.json`);
  try {
    const entries = JSON.parse(await readFile(cache, "utf8")) as [string, string][];
    if (Array.isArray(entries) && entries.every((entry) => Array.isArray(entry) && entry.length === 2 && entry.every((item) => typeof item === "string"))) return new Map(entries);
  } catch { /* Fetch the matching language file when the cache is absent. */ }
  const manifest = JSON.parse((await mojangBytes("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json", 2 * 1024 * 1024)).toString("utf8")) as { versions: { id: string; url: string }[] };
  const versionUrl = manifest.versions.find((item) => item.id === version)?.url;
  if (!versionUrl) throw new Error(`Minecraft version ${version} was not found`);
  const details = JSON.parse((await mojangBytes(versionUrl, 1024 * 1024)).toString("utf8")) as { downloads: { client: { url: string } } };
  const language = JSON.parse(zipEntry(await mojangBytes(details.downloads.client.url, 120 * 1024 * 1024), "assets/minecraft/lang/en_us.json")) as Record<string, unknown>;
  const descriptions = new Map<string, string>();
  for (const [key, title] of Object.entries(language)) {
    if (!/^advancements\..+\.title$/.test(key) || typeof title !== "string") continue;
    const description = language[key.replace(/\.title$/, ".description")];
    if (typeof description === "string") descriptions.set(title, description);
  }
  await writeFile(cache, JSON.stringify([...descriptions])).catch(() => {});
  return descriptions;
}

function descriptionFor(version: string | null | undefined, title: string) {
  if (!version || !/^[A-Za-z0-9._-]{1,50}$/.test(version)) return Promise.resolve(null);
  let pending = advancementDescriptions.get(version);
  if (!pending) {
    pending = loadAdvancementDescriptions(version);
    advancementDescriptions.set(version, pending);
    void pending.catch(() => advancementDescriptions.delete(version));
  }
  return pending.then((descriptions) => descriptions.get(title) ?? null);
}

function versionFor(server: Server) {
  if (server.minecraft_version && /^[A-Za-z0-9._-]{1,50}$/.test(server.minecraft_version)) return Promise.resolve(server.minecraft_version);
  let pending = serverVersions.get(server.id);
  if (!pending) {
    pending = (async () => {
      const file = await open(join(server.directory, "logs", "latest.log"), "r");
      try {
        const bytes = Buffer.alloc(64 * 1024);
        const { bytesRead } = await file.read(bytes, 0, bytes.length, 0);
        return /Starting minecraft server version ([A-Za-z0-9._-]{1,50})/.exec(bytes.toString("utf8", 0, bytesRead))?.[1] ?? null;
      } finally { await file.close(); }
    })().catch(() => null);
    serverVersions.set(server.id, pending);
    void pending.then((version) => { if (!version) serverVersions.delete(server.id); });
  }
  return pending;
}

async function hasSession(name: string) {
  try {
    const { stdout } = await run("tmux", ["display-message", "-p", "-t", name, "#{pane_dead}"], { timeout: 5000 });
    return stdout.trim() === "0";
  }
  catch { return false; }
}

function modernClickEvents(version?: string | null) {
  const match = /^(\d+)\.(\d+)(?:\.(\d+))?/.exec(version ?? "");
  if (!match) return true;
  const [, major, minor, patch] = match.map(Number);
  return major > 1 || major === 1 && (minor > 21 || minor === 21 && patch >= 5);
}

function siteName(url: URL) {
  const host = url.hostname.toLowerCase().replace(/^www\./, "");
  for (const [domain, name] of Object.entries({ "youtube.com": "YouTube", "youtu.be": "YouTube", "tenor.com": "Tenor", "giphy.com": "Giphy", "tiktok.com": "TikTok", "twitch.tv": "Twitch", "reddit.com": "Reddit", "discord.com": "Discord", "discordapp.com": "Discord", "discordapp.net": "Discord", "instagram.com": "Instagram", "x.com": "X", "twitter.com": "X" })) {
    if (host === domain || host.endsWith(`.${domain}`)) return name;
  }
  return host;
}

function linkLabel(label: string | null | undefined, fallback: string) {
  return clean(label?.replace(/[\[\]]/g, "") ?? "", 60) || fallback;
}

function minecraftChat(template: string, username: string, name: string, content: string, attachments: AttachmentLink[], linkLabels: Map<string, string>, modern: boolean) {
  const extra: { text: string; color?: string; underlined?: boolean; click_event?: { action: string; url: string }; clickEvent?: { action: string; value: string } }[] = [];
  const tokens = /\{(username|name|message|color:[^{}]+)\}/g;
  let color: string | undefined;
  let offset = 0;
  let remaining = 400;
  function add(value: string) {
    if (remaining <= 0) return;
    const text = value.replace(/[\u0000-\u001f\u007f]/g, " ").replace(/\s+/g, " ").slice(0, remaining);
    if (text) { extra.push(color ? { text, color } : { text }); remaining -= text.length; }
  }
  function addLink(text: string, url: string, limited = false) {
    if (limited && remaining < text.length) return;
    if (limited) remaining -= text.length;
    const link = { text, color: "aqua", underlined: true };
    extra.push(modern
      ? { ...link, click_event: { action: "open_url", url } }
      : { ...link, clickEvent: { action: "open_url", value: url } });
  }
  function addMessage(value: string) {
    const urls = /https?:\/\/[^\s<>]+/gi;
    let offset = 0;
    let linkCount = 0;
    for (const match of value.matchAll(urls)) {
      add(value.slice(offset, match.index));
      const raw = match[0];
      const link = raw.replace(/[.,!?;:)\]}]+$/, "");
      try {
        const url = new URL(link);
        if ((url.protocol === "http:" || url.protocol === "https:") && url.href.length <= 2048 && linkCount < 10) {
          addLink(`[${linkLabels.get(url.href) ?? siteName(url)}]`, url.href, true);
          linkCount++;
        }
        else add(link);
      } catch { add(link); }
      add(raw.slice(link.length));
      offset = match.index + raw.length;
    }
    add(value.slice(offset));
  }
  for (const match of template.matchAll(tokens)) {
    add(template.slice(offset, match.index));
    const token = match[1];
    if (token === "username") add(username);
    else if (token === "name") add(name);
    else if (token === "message") addMessage(content);
    else {
      const next = token.slice("color:".length).toLowerCase();
      if (next === "reset") color = undefined;
      else if (minecraftColors.has(next) || /^#[0-9a-f]{6}$/.test(next)) color = next;
      else add(match[0]);
    }
    offset = match.index + match[0].length;
  }
  add(template.slice(offset));
  for (const attachment of attachments) {
    const separator = extra.length && !/\s$/.test(extra.at(-1)?.text ?? "") ? " " : "";
    addLink(`${separator}[${attachment.label}]`, attachment.url);
  }
  return { text: "", extra };
}

async function sendToMinecraft(server: Server, username: string, name: string, content: string, attachments: AttachmentLink[], linkLabels: Map<string, string>) {
  let available = false;
  for (let attempt = 0; attempt < 3; attempt++) {
    if (await hasSession(server.session)) { available = true; break; }
    if (attempt < 2) await new Promise((resolve) => setTimeout(resolve, 500));
  }
  if (!available) throw new Error("Minecraft console was unavailable after three checks");
  const savedTemplate = server.chat_to_minecraft_template;
  const template = !savedTemplate || legacyMinecraftTemplates.has(savedTemplate) ? defaultMinecraftTemplate : savedTemplate;
  const command = `tellraw @a ${JSON.stringify(minecraftChat(template, username, name, content, attachments, linkLabels, modernClickEvents(server.minecraft_version)))}`;
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
    const embed = new EmbedBuilder().setTitle(title).setColor(color);
    if (description) embed.setDescription(description);
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
          const achievement = line.match(/\]: ([A-Za-z0-9_]{1,16}) has (made the advancement|completed the challenge|reached the goal) \[([^\]]{1,100})\]$/);
          if (achievement) {
            if (!server.achievements || !channelFor(server, "achievements")) continue;
            const [, player, kind, name] = achievement;
            const color = kind === "completed the challenge" ? 0xaa00aa : kind === "reached the goal" ? 0xffaa00 : 0x55aa55;
            void versionFor(server).then((version) => descriptionFor(version, name))
              .catch((error) => { console.error(`Could not read advancement descriptions for ${server.name}: ${String(error)}`); return null; })
              .then((description) => announceEmbed(server, "achievements", `${player} has ${kind} ${clean(name, 100)}`, description ?? "", color));
            continue;
          }
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
  if (!message.content.trim() && !message.attachments.size && !message.embeds.length) return;
  const content = clean(message.content.replace(/<(?:a:|:)?([A-Za-z0-9_]+):\d+>/g, ":$1:"), 2000);
  const linkLabels = new Map<string, string>();
  for (const embed of message.embeds) {
    if (!embed.url) continue;
    try {
      const url = new URL(embed.url);
      if (url.protocol === "http:" || url.protocol === "https:") {
        linkLabels.set(url.href, linkLabel(embed.title || (embed.data.type === "gifv" ? embed.description : null), siteName(url)));
      }
    } catch { /* Ignore invalid embed URLs. */ }
  }
  const attachments = message.attachments.map((attachment) => {
    const type = attachment.contentType ?? "";
    const label = type === "image/gif" || /\.gif$/i.test(attachment.name) ? "GIF"
      : type.startsWith("image/") || /\.(?:png|jpe?g|webp|avif|bmp)$/i.test(attachment.name) ? "image"
      : type.startsWith("video/") || /\.(?:mp4|mov|webm)$/i.test(attachment.name) ? "video"
      : type.startsWith("audio/") || /\.(?:mp3|ogg|wav|m4a)$/i.test(attachment.name) ? "audio" : "file";
    const url = attachment.proxyURL || attachment.url;
    try { return new URL(url).protocol === "https:" ? { label: linkLabel(attachment.description, label), url } : null; }
    catch { return null; }
  }).filter((attachment): attachment is AttachmentLink => attachment !== null);
  if (!attachments.length && !/https?:\/\//i.test(content)) {
    for (const embed of message.embeds) {
      if (!embed.url) continue;
      try {
        const url = new URL(embed.url);
        if (url.protocol === "http:" || url.protocol === "https:") attachments.push({ label: linkLabels.get(url.href) ?? siteName(url), url: url.href });
      } catch { /* Ignore invalid embed URLs. */ }
    }
  }
  if (!content && !attachments.length) return;
  for (const server of servers.filter((item) => item.chat_to_minecraft && channelFor(item, "chat_to_minecraft") === message.channel.id)) {
    void sendToMinecraft(server, clean(message.author.username, 40), clean(message.author.displayName, 40), content, attachments, linkLabels)
      .catch((error) => console.error(`Could not deliver Discord message ${message.id} to ${server.name}: ${String(error)}`));
  }
});
client.once(Events.ClientReady, () => { void watchServers(); });
client.on(Events.Error, (error) => console.error(`Discord connection error: ${String(error)}`));
try { await client.login(token); }
catch { console.error("Discord login failed. Check the bot token and Message Content intent."); process.exitCode = 1; }
