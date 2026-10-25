export interface FavoritePackage {
  kind: "mod" | "plugin";
  id: string;
  title: string;
  description: string;
  author: string;
  iconUrl: string | null;
  downloads: number;
  badges?: ("required" | "optional" | "server")[];
}

const storageKey = "scerite.packageFavorites.v1";

export function readPackageFavorites(): FavoritePackage[] {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(storageKey) ?? "[]");
    if (!Array.isArray(saved)) return [];
    return saved.slice(0, 500).filter((item): item is FavoritePackage => item && typeof item === "object"
      && (item.kind === "mod" || item.kind === "plugin") && typeof item.id === "string" && item.id.length > 0
      && typeof item.title === "string" && typeof item.description === "string"
      && typeof item.author === "string" && (item.iconUrl === null || typeof item.iconUrl === "string")
      && typeof item.downloads === "number" && Number.isFinite(item.downloads)
      && (item.badges === undefined || Array.isArray(item.badges)
        && item.badges.every((badge: unknown) => badge === "required" || badge === "optional" || badge === "server")));
  } catch { return []; }
}

export function savePackageFavorites(favorites: FavoritePackage[]) {
  localStorage.setItem(storageKey, JSON.stringify(favorites));
}
