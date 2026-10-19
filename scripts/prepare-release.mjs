import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";

function parseVersion(value) {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(value);
  if (!match) throw new Error(`Expected a stable X.Y.Z version, got ${value}`);
  return match.slice(1).map(Number);
}

function compareVersions(left, right) {
  for (let index = 0; index < 3; index += 1) {
    if (left[index] !== right[index]) return left[index] - right[index];
  }
  return 0;
}

function replacePackageVersion(path, version) {
  const source = readFileSync(path, "utf8");
  const pattern = /(\[\[?package\]\]?\r?\nname = "scerite"\r?\nversion = ")[^"]+(")/;
  if (!pattern.test(source)) throw new Error(`Could not find the scerite version in ${path}`);
  writeFileSync(path, source.replace(pattern, (_, prefix, suffix) => `${prefix}${version}${suffix}`));
}

const packageJson = JSON.parse(readFileSync("package.json", "utf8"));
const current = parseVersion(packageJson.version);
const tags = execFileSync("git", ["tag", "--list", "v*"], { encoding: "utf8" })
  .split(/\r?\n/)
  .filter((tag) => /^v\d+\.\d+\.\d+$/.test(tag))
  .map((tag) => parseVersion(tag.slice(1)));
const latest = tags.sort(compareVersions).at(-1);
const matchingTag = `v${packageJson.version}`;
const tagOnHead = execFileSync("git", ["tag", "--points-at", "HEAD", "--list", matchingTag], { encoding: "utf8" }).trim() === matchingTag;

let version = packageJson.version;
if (!tagOnHead && latest && compareVersions(current, latest) <= 0) {
  version = `${latest[0]}.${latest[1]}.${latest[2] + 1}`;
}

const changelog = readFileSync("CHANGELOG.md", "utf8");
const unreleased = /^## \[Unreleased\]\r?\n/m.exec(changelog);
if (!unreleased) throw new Error("CHANGELOG.md needs an Unreleased section.");
const notesStart = unreleased.index + unreleased[0].length;
const following = changelog.slice(notesStart);
const nextSection = /^## \[/m.exec(following);
const notes = following.slice(0, nextSection?.index ?? following.length).trim();
if (!/^[-*] /m.test(notes)) throw new Error("Add release notes under Unreleased in CHANGELOG.md before merging.");
if (new RegExp(`^## \\[${version.replaceAll(".", "\\.")}\\]`, "m").test(changelog)) {
  throw new Error(`CHANGELOG.md already has notes for ${version}.`);
}

if (version !== packageJson.version) {
  packageJson.version = version;
  writeFileSync("package.json", `${JSON.stringify(packageJson, null, 2)}\n`);

  const packageLock = JSON.parse(readFileSync("package-lock.json", "utf8"));
  packageLock.version = version;
  packageLock.packages[""].version = version;
  writeFileSync("package-lock.json", `${JSON.stringify(packageLock, null, 2)}\n`);

  replacePackageVersion("src-tauri/Cargo.toml", version);
  replacePackageVersion("src-tauri/Cargo.lock", version);

  const tauriConfig = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
  tauriConfig.version = version;
  writeFileSync("src-tauri/tauri.conf.json", `${JSON.stringify(tauriConfig, null, 2)}\n`);
}

const history = nextSection ? following.slice(nextSection.index) : "";
const date = new Date().toISOString().slice(0, 10);
writeFileSync("CHANGELOG.md", `${changelog.slice(0, unreleased.index)}## [Unreleased]\n\n## [${version}] - ${date}\n\n${notes}\n\n${history}`);

console.log(`version=${version}`);
console.log(`tag=v${version}`);
