import * as TOML from "smol-toml";
import { parseDocument } from "yaml";

export type ConfigFormat = "json" | "yaml" | "toml" | "properties" | "raw";
export type ConfigValue = string | number | boolean;
export interface ConfigField { path: (string | number)[]; label: string; value: ConfigValue; }

export function configFormat(path: string): ConfigFormat {
  const name = path.toLowerCase();
  if (name.endsWith(".json")) return "json";
  if (name.endsWith(".yml") || name.endsWith(".yaml")) return "yaml";
  if (name.endsWith(".toml")) return "toml";
  if (name.endsWith(".properties")) return "properties";
  return "raw";
}

function propertyLines(source: string) {
  const lines = source.split(/\r?\n/);
  return lines.flatMap((line, index) => {
    if (/^\s*[#!]/.test(line) || /\\\s*$/.test(line)) return [];
    const match = /^(\s*([^=:]+?)\s*[:=]\s*)(.*)$/.exec(line);
    return match && match[2].trim() ? [{ path: [index], label: match[2].trim(), value: match[3] }] : [];
  });
}

function parsedValue(format: ConfigFormat, source: string): unknown {
  if (format === "json") return JSON.parse(source);
  if (format === "toml") return TOML.parse(source, { unsafeKeyBehaviour: "throw", integersAsBigInt: "asNeeded" });
  const document = parseDocument(source, { uniqueKeys: true });
  if (document.errors.length) throw new Error(document.errors[0].message);
  return document.toJS({ maxAliasCount: 100 });
}

export function configFields(path: string, source: string): ConfigField[] {
  const format = configFormat(path);
  if (format === "raw") return [];
  if (format === "properties") return propertyLines(source);
  const fields: ConfigField[] = [];
  const seen = new Set<object>();
  function visit(value: unknown, parts: (string | number)[], depth: number) {
    if (fields.length >= 300 || depth > 8) return;
    if (typeof value === "string" || typeof value === "boolean" || typeof value === "number" && Number.isFinite(value)) {
      fields.push({ path: parts, label: parts.join(" / "), value });
    } else if (value && typeof value === "object" && !(value instanceof Date) && !seen.has(value)) {
      seen.add(value);
      for (const [key, child] of Object.entries(value)) visit(child, [...parts, Array.isArray(value) ? Number(key) : key], depth + 1);
    }
  }
  visit(parsedValue(format, source), [], 0);
  return fields;
}

export function applyConfigEdits(path: string, source: string, edits: Record<string, ConfigValue>): string {
  const format = configFormat(path);
  if (format === "raw") throw new Error("This file can only be edited in Raw view.");
  if (!Object.keys(edits).length) return source;
  const fields = configFields(path, source);
  const byPath = new Map(fields.map((field) => [JSON.stringify(field.path), field]));
  const replacements = Object.entries(edits).map(([key, value]) => {
    const field = byPath.get(key);
    if (!field) throw new Error("A configuration field changed. Reopen the file before saving.");
    let next: ConfigValue = value;
    if (typeof field.value === "number") {
      next = Number(value);
      if (!Number.isFinite(next) || String(value).trim() === "") throw new Error(`${field.label} needs a valid number.`);
    } else if (typeof field.value === "boolean") next = value === true;
    else next = String(value);
    return { path: field.path, value: next };
  });
  if (format === "properties") {
    const newline = source.includes("\r\n") ? "\r\n" : "\n";
    const lines = source.split(/\r?\n/);
    for (const { path: [index], value } of replacements) {
      const line = lines[Number(index)];
      const match = /^(\s*[^=:]+?\s*[:=]\s*)(.*)$/.exec(line);
      if (!match) throw new Error("A property changed. Reopen the file before saving.");
      lines[Number(index)] = `${match[1]}${value}`;
    }
    return lines.join(newline);
  }
  if (format === "yaml") {
    const document = parseDocument(source, { uniqueKeys: true });
    if (document.errors.length) throw new Error(document.errors[0].message);
    for (const { path: parts, value } of replacements) document.setIn(parts, value);
    return document.toString();
  }
  const parsed = parsedValue(format, source);
  for (const { path: parts, value } of replacements) {
    let parent = parsed as Record<string | number, unknown>;
    for (const part of parts.slice(0, -1)) {
      if (!parent || typeof parent !== "object" || !Object.prototype.hasOwnProperty.call(parent, part)) throw new Error("A configuration field changed. Reopen the file before saving.");
      parent = parent[part] as Record<string | number, unknown>;
    }
    const last = parts[parts.length - 1];
    if (last === undefined || !parent || typeof parent !== "object" || !Object.prototype.hasOwnProperty.call(parent, last)) throw new Error("A configuration field changed. Reopen the file before saving.");
    Object.defineProperty(parent, last, { value, writable: true, enumerable: true, configurable: true });
  }
  return format === "json" ? `${JSON.stringify(parsed, null, 2)}\n` : TOML.stringify(parsed as TOML.TomlTable);
}
