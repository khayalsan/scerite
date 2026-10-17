import { useEffect, useState } from "react";
import { RotateCcw, Save } from "lucide-react";
import { serverFiles } from "../lib/serverFiles";

type PropertyEntry = {
  key: string;
  lineIndex: number;
  prefix: string;
  separator: string;
  leadingWhitespace: string;
  trailingWhitespace: string;
  value: string;
};

type PropertyDocument = {
  lines: string[];
  entries: PropertyEntry[];
  newline: string;
  trailingNewline: boolean;
};

type PropertyInfo = {
  label: string;
  description: string;
  group: string;
};

const propertyInfo: Record<string, PropertyInfo> = {
  motd: { label: "Message of the day", description: "Shown beneath the server name in the multiplayer list.", group: "General" },
  "level-name": { label: "World folder", description: "The folder used for the server world.", group: "General" },
  "server-ip": { label: "Bind address", description: "Leave blank to accept connections on all network interfaces.", group: "Network" },
  "server-port": { label: "Server port", description: "The port players use to connect.", group: "Network" },
  "max-players": { label: "Maximum players", description: "The maximum number of players allowed at once.", group: "Players" },
  "online-mode": { label: "Online authentication", description: "Verify players with Minecraft account services.", group: "Players" },
  "white-list": { label: "Whitelist", description: "Only allow players on the whitelist to join.", group: "Players" },
  "enforce-whitelist": { label: "Enforce whitelist", description: "Disconnect players who are not on the whitelist.", group: "Players" },
  "hide-online-players": { label: "Hide online players", description: "Do not show the player list in server status responses.", group: "Players" },
  difficulty: { label: "Difficulty", description: "The default difficulty for the world.", group: "Gameplay" },
  gamemode: { label: "Default game mode", description: "The game mode assigned to new players.", group: "Gameplay" },
  pvp: { label: "PVP", description: "Allow players to damage one another.", group: "Gameplay" },
  "allow-flight": { label: "Allow flight", description: "Allow players to fly with supported mods or permissions.", group: "Gameplay" },
  "allow-nether": { label: "Allow Nether", description: "Allow players to travel to the Nether.", group: "Gameplay" },
  "enable-command-block": { label: "Command blocks", description: "Allow command blocks to run commands.", group: "Gameplay" },
  "spawn-monsters": { label: "Spawn monsters", description: "Allow hostile mobs to spawn.", group: "Gameplay" },
  "spawn-animals": { label: "Spawn animals", description: "Allow passive mobs to spawn.", group: "Gameplay" },
  "spawn-npcs": { label: "Spawn villagers", description: "Allow villagers to spawn.", group: "Gameplay" },
  "force-gamemode": { label: "Force default game mode", description: "Set each player to the default game mode when they join.", group: "Gameplay" },
  "view-distance": { label: "View distance", description: "The number of world chunks sent to each player.", group: "World & performance" },
  "simulation-distance": { label: "Simulation distance", description: "The number of chunks around players where entities are active.", group: "World & performance" },
  "spawn-protection": { label: "Spawn protection", description: "Protected radius around the world spawn point.", group: "World & performance" },
  "max-world-size": { label: "Maximum world size", description: "The maximum border radius in blocks.", group: "World & performance" },
  "level-seed": { label: "World seed", description: "Seed used when generating a new world.", group: "World & performance" },
  "level-type": { label: "World type", description: "World generation preset used for a new world.", group: "World & performance" },
  "generate-structures": { label: "Generate structures", description: "Generate structures such as villages in new chunks.", group: "World & performance" },
  "player-idle-timeout": { label: "Idle timeout", description: "Minutes before an idle player is kicked. Use 0 to disable.", group: "Players" },
};

const numericProperties = new Set([
  "server-port",
  "max-players",
  "view-distance",
  "simulation-distance",
  "spawn-protection",
  "max-world-size",
  "player-idle-timeout",
]);

const selectOptions: Record<string, string[]> = {
  difficulty: ["peaceful", "easy", "normal", "hard"],
  gamemode: ["survival", "creative", "adventure", "spectator"],
};

const groupOrder = ["General", "Players", "Gameplay", "World & performance", "Network", "Other settings"];

function parseProperties(content: string): PropertyDocument {
  const newline = content.includes("\r\n") ? "\r\n" : content.includes("\r") ? "\r" : "\n";
  const trailingNewline = /(?:\r\n|\n|\r)$/.test(content);
  const lines = content.split(/\r\n|\n|\r/);
  if (trailingNewline) lines.pop();

  const entries: PropertyEntry[] = [];
  lines.forEach((line, lineIndex) => {
    const trimmed = line.trimStart();
    if (!trimmed || trimmed.startsWith("#") || trimmed.startsWith("!")) return;

    const separatorIndex = line.search(/[=:]/);
    const key = line.slice(0, separatorIndex < 0 ? line.length : separatorIndex).trim();
    if (!key) return;

    const prefix = separatorIndex < 0 ? line : line.slice(0, separatorIndex);
    const separator = separatorIndex < 0 ? "" : line[separatorIndex];
    const rawValue = separatorIndex < 0 ? "" : line.slice(separatorIndex + 1);
    const leadingWhitespace = rawValue.match(/^\s*/)?.[0] ?? "";
    let trailingWhitespace = rawValue.match(/\s*$/)?.[0] ?? "";
    if (leadingWhitespace.length + trailingWhitespace.length > rawValue.length) trailingWhitespace = "";
    const valueEnd = rawValue.length - trailingWhitespace.length;

    entries.push({
      key,
      lineIndex,
      prefix,
      separator,
      leadingWhitespace,
      trailingWhitespace,
      value: rawValue.slice(leadingWhitespace.length, Math.max(leadingWhitespace.length, valueEnd)),
    });
  });

  return { lines, entries, newline, trailingNewline };
}

function serializeProperties(document: PropertyDocument, values: Record<string, string>) {
  const updatedLines = document.lines.map((line, index) => {
    const entry = document.entries.find((property) => property.lineIndex === index);
    if (!entry) return line;
    const value = values[entry.key] ?? entry.value;
    if (!entry.separator && value === entry.value) return line;
    return `${entry.prefix}${entry.separator || "="}${entry.leadingWhitespace}${value}${entry.trailingWhitespace}`;
  });
  return `${updatedLines.join(document.newline)}${document.trailingNewline ? document.newline : ""}`;
}

function displayName(key: string) {
  return key.split("-").map((part) => part ? `${part[0].toUpperCase()}${part.slice(1)}` : part).join(" ");
}

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

type ServerPropertiesPanelProps = {
  profileId: string;
  remote: boolean;
  canEdit: boolean;
};

export function ServerPropertiesPanel({ profileId, remote, canEdit }: ServerPropertiesPanelProps) {
  const [document, setDocument] = useState<PropertyDocument | null>(null);
  const [values, setValues] = useState<Record<string, string>>({});
  const [originalValues, setOriginalValues] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);
  const [errorAction, setErrorAction] = useState<"load" | "save" | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [reloadKey, setReloadKey] = useState(0);
  const hasChanges = Object.keys(values).some((key) => values[key] !== originalValues[key]);

  useEffect(() => {
    let active = true;
    setIsLoading(true);
    setError(null);
    setErrorAction(null);
    void serverFiles.read(remote, profileId, "server.properties").then((content) => {
      if (!active) return;
      const parsed = parseProperties(content);
      const nextValues = Object.fromEntries(parsed.entries.map((entry) => [entry.key, entry.value]));
      setDocument(parsed);
      setValues(nextValues);
      setOriginalValues(nextValues);
    }).catch((reason: unknown) => {
      if (!active) return;
      setError(errorText(reason));
      setErrorAction("load");
    }).finally(() => {
      if (active) setIsLoading(false);
    });
    return () => { active = false; };
  }, [profileId, remote, reloadKey]);

  async function saveProperties() {
    if (!document || !hasChanges || !canEdit || isSaving) return;
    if (!window.confirm("Save these changes to server.properties? The existing file will be overwritten.")) return;

    setIsSaving(true);
    setError(null);
    try {
      const content = serializeProperties(document, values);
      await serverFiles.write(remote, profileId, "server.properties", content, true);
      const savedDocument = parseProperties(content);
      setDocument(savedDocument);
      setValues(Object.fromEntries(savedDocument.entries.map((entry) => [entry.key, entry.value])));
      setOriginalValues(Object.fromEntries(savedDocument.entries.map((entry) => [entry.key, entry.value])));
    } catch (reason) {
      setError(errorText(reason));
      setErrorAction("save");
    } finally {
      setIsSaving(false);
    }
  }

  function retryError() {
    if (errorAction === "save") void saveProperties();
    else setReloadKey((current) => current + 1);
  }

  function resetChanges() {
    if (hasChanges && window.confirm("Discard unsaved server.properties changes?")) {
      setValues(originalValues);
      setError(null);
    }
  }

  const groups = document?.entries.reduce<Record<string, PropertyEntry[]>>((result, entry) => {
    const group = propertyInfo[entry.key]?.group ?? "Other settings";
    (result[group] ??= []).push(entry);
    return result;
  }, {});

  return (
    <div className="server-properties-panel" aria-label="Server properties editor">
      <header className="properties-header">
        <div>
          <h2>Server properties</h2>
          <p>Edit the settings stored in <code>server.properties</code>.</p>
        </div>
        <div className="properties-actions">
          {hasChanges && <span className="properties-unsaved" role="status">Unsaved changes</span>}
          <button className="button button-quiet" type="button" onClick={resetChanges} disabled={!hasChanges || isSaving}>
            <RotateCcw size={15} aria-hidden="true" /> Reset
          </button>
          <button className="button button-primary" type="button" onClick={() => void saveProperties()} disabled={!hasChanges || !canEdit || isSaving || isLoading}>
            <Save size={15} aria-hidden="true" /> {isSaving ? "Saving…" : "Save changes"}
          </button>
        </div>
      </header>

      {!canEdit && (
        <div className="properties-notice" role="status">
          <strong>Stop the server to edit properties.</strong>
          <span>Settings can be saved after the server has stopped.</span>
        </div>
      )}
      {error && (
        <div className="notice notice-error" role="alert">
          <div><strong>{errorAction === "save" ? "Could not save server.properties." : "Could not load server.properties."}</strong><span>{error}</span></div>
          <button className="button button-quiet" type="button" onClick={retryError} disabled={isLoading || isSaving}>{errorAction === "save" ? "Retry save" : "Retry"}</button>
        </div>
      )}
      {isLoading ? (
        <p className="properties-state" role="status">Loading server properties…</p>
      ) : groups && Object.keys(groups).length > 0 ? (
        <div className="properties-groups">
          {groupOrder.filter((group) => groups[group]).map((group) => (
            <section className="property-group" key={group}>
              <h3>{group}</h3>
              <div className="property-fields">
                {groups[group].map((entry) => {
                  const value = values[entry.key] ?? "";
                  const info = propertyInfo[entry.key];
                  const booleanValue = value.toLowerCase();
                  const isBoolean = booleanValue === "true" || booleanValue === "false";
                  const options = selectOptions[entry.key];
                  const label = info?.label ?? displayName(entry.key);
                  const isNumeric = numericProperties.has(entry.key) || (!info && /^-?\d+(?:\.\d+)?$/.test(value));

                  return (
                    <div className={`property-field${isBoolean ? " property-field-switch" : ""}`} key={`${entry.key}-${entry.lineIndex}`}>
                      <div className="property-field-copy">
                        <label htmlFor={`property-${entry.lineIndex}`}>{label}</label>
                        <small>{info?.description ?? entry.key}</small>
                      </div>
                      {isBoolean ? (
                        <label className="property-switch" aria-label={label}>
                          <input
                            id={`property-${entry.lineIndex}`}
                            type="checkbox"
                            role="switch"
                            checked={booleanValue === "true"}
                            disabled={!canEdit || isSaving}
                            onChange={(event) => setValues((current) => ({ ...current, [entry.key]: String(event.currentTarget.checked) }))}
                          />
                          <span aria-hidden="true" />
                        </label>
                      ) : options ? (
                        <select
                          id={`property-${entry.lineIndex}`}
                          className="property-control"
                          value={value}
                          disabled={!canEdit || isSaving}
                          onChange={(event) => setValues((current) => ({ ...current, [entry.key]: event.currentTarget.value }))}
                        >
                          {!options.includes(value) && <option value={value}>{value}</option>}
                          {options.map((option) => <option key={option} value={option}>{displayName(option)}</option>)}
                        </select>
                      ) : (
                        <input
                          id={`property-${entry.lineIndex}`}
                          className="property-control"
                          type={isNumeric ? "number" : "text"}
                          step={isNumeric ? "any" : undefined}
                          value={value}
                          disabled={!canEdit || isSaving}
                          onChange={(event) => setValues((current) => ({ ...current, [entry.key]: event.currentTarget.value }))}
                        />
                      )}
                    </div>
                  );
                })}
              </div>
            </section>
          ))}
        </div>
      ) : !error ? (
        <div className="properties-state">
          <strong>No properties found</strong>
          <span>Start the server once to generate its <code>server.properties</code> file.</span>
        </div>
      ) : null}
    </div>
  );
}
