import { readFileSync } from "node:fs";

const tag = process.argv[2];
if (!/^v\d+\.\d+\.\d+$/.test(tag ?? "")) throw new Error("Pass a release tag such as v0.3.2.");

const changelog = readFileSync("CHANGELOG.md", "utf8");
const heading = new RegExp(`^## \\[${tag.slice(1).replaceAll(".", "\\.")}\\] - \\d{4}-\\d{2}-\\d{2}\\r?\\n`, "m").exec(changelog);
if (!heading) throw new Error(`CHANGELOG.md has no notes for ${tag}.`);
const following = changelog.slice(heading.index + heading[0].length);
const nextSection = /^## \[/m.exec(following);
const notes = following.slice(0, nextSection?.index ?? following.length).trim();
if (!notes) throw new Error(`CHANGELOG.md has empty notes for ${tag}.`);
process.stdout.write(`${notes}\n`);
