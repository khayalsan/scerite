import type { ServerFileEntry } from "./serverFiles";
import type { ServerProfile } from "./serverProfiles";

export type PackageKind = "mod" | "plugin";
export type PackageSupport = { mods: boolean; plugins: boolean };

export function packageSupportForSoftware(software: string | null | undefined): PackageSupport | null {
  if (!software) return null;
  if (software.startsWith("arclight-")) return { mods: true, plugins: true };
  if (["fabric", "quilt", "forge", "neoforge"].includes(software)) return { mods: true, plugins: false };
  if (["paper", "purpur", "spigot", "bukkit", "folia"].includes(software)) return { mods: false, plugins: true };
  return { mods: false, plugins: false };
}

export function packageLoader(kind: PackageKind, software: string | null | undefined): string | null {
  if (!software) return null;
  if (software.startsWith("arclight-")) return kind === "plugin" ? "bukkit" : software.slice("arclight-".length);
  if (kind === "mod" && ["fabric", "quilt", "forge", "neoforge"].includes(software)) return software;
  if (kind === "plugin" && ["paper", "purpur", "spigot", "bukkit", "folia"].includes(software)) return software;
  return null;
}

export function packageSupportFromFiles(profile: ServerProfile, entries: ServerFileEntry[]): PackageSupport {
  const names = new Set(entries.map((entry) => entry.name.toLowerCase()));
  const directories = new Set(entries.filter((entry) => entry.directory).map((entry) => entry.name.toLowerCase()));
  const launch = profile.target.launch.arguments.join(" ").toLowerCase();
  if ([...names].some((name) => name.startsWith("arclight-") && name.endsWith(".jar")) || launch.includes("arclight-")) {
    return { mods: true, plugins: true };
  }
  const mods = directories.has("mods") || names.has("fabric-server-launch.jar") || names.has("quilt-server-launch.jar")
    || launch.includes("@libraries/net/minecraftforge/") || launch.includes("@libraries/net/neoforged/");
  const plugins = directories.has("plugins");
  return { mods, plugins };
}
