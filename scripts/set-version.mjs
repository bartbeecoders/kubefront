#!/usr/bin/env node
// Single source of truth for the app version. Writes ONE version into all three
// manifests that must agree: package.json, src-tauri/tauri.conf.json, and
// src-tauri/Cargo.toml ([package] version).
//
// Usage:
//   node scripts/set-version.mjs 0.2.3        # write an explicit version
//   node scripts/set-version.mjs --from-tag   # derive from git tag (GITHUB_REF_NAME or `git describe`)
//   node scripts/set-version.mjs --check       # verify all three already match (no writes); exit 1 if not
//   node scripts/set-version.mjs 0.2.3 --commit       # also `git commit` the three files
//   node scripts/set-version.mjs 0.2.3 --commit --tag # also create the vX.Y.Z git tag
//
// The version is normalised: a leading "v" is stripped, and it must look like
// X.Y.Z with an optional -prerelease suffix (e.g. 0.2.3 or 1.0.0-rc.1).

import { readFileSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const PKG = join(ROOT, "package.json");
const CONF = join(ROOT, "src-tauri", "tauri.conf.json");
const CARGO = join(ROOT, "src-tauri", "Cargo.toml");
// Workspace lockfile lives at the repo root (the workspace produces ONE Cargo.lock).
const CARGO_LOCK = join(ROOT, "Cargo.lock");
const CRATE = "kube-front"; // [package] name, used to target the right Cargo.lock entry

const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

function die(msg) {
  console.error(`set-version: ${msg}`);
  process.exit(1);
}

function normalize(raw) {
  const v = String(raw).trim().replace(/^v/, "");
  if (!SEMVER.test(v)) die(`"${raw}" is not a valid X.Y.Z version`);
  return v;
}

function readJsonVersion(path) {
  return JSON.parse(readFileSync(path, "utf8")).version;
}

function readCargoVersion() {
  const m = readFileSync(CARGO, "utf8").match(/\[package\][^[]*?\bversion\s*=\s*"([^"]*)"/);
  return m ? m[1] : null;
}

function readCargoLockVersion() {
  const m = readFileSync(CARGO_LOCK, "utf8").match(new RegExp(`name = "${CRATE}"\\r?\\nversion = "([^"]*)"`));
  return m ? m[1] : null;
}

function writeJsonVersion(path, version) {
  const json = JSON.parse(readFileSync(path, "utf8"));
  json.version = version;
  writeFileSync(path, JSON.stringify(json, null, 2) + "\n");
}

function writeCargoVersion(version) {
  const text = readFileSync(CARGO, "utf8");
  // Only the version inside the [package] table — never a dependency's version.
  // NOTE: match-test explicitly; comparing replace output to the input would
  // false-fail when the file already holds the target version (e.g. a CI
  // checkout of a tag whose manifests were committed pre-stamped).
  const re = /(\[package\][^[]*?\bversion\s*=\s*")[^"]*(")/;
  if (!re.test(text)) die("could not find [package] version in Cargo.toml");
  writeFileSync(CARGO, text.replace(re, `$1${version}$2`));
}

function writeCargoLockVersion(version) {
  // Keep the lockfile's own crate entry in sync so a bump doesn't leave Cargo.lock
  // dirty until the next build. Targets the `name = "kube-front"` block specifically.
  const text = readFileSync(CARGO_LOCK, "utf8");
  const re = new RegExp(`(name = "${CRATE}"\\r?\\nversion = ")[^"]*(")`);
  if (!re.test(text)) die(`could not find ${CRATE} entry in Cargo.lock`);
  writeFileSync(CARGO_LOCK, text.replace(re, `$1${version}$2`));
}

function git(args) {
  return execFileSync("git", args, { cwd: ROOT, encoding: "utf8" }).trim();
}

function fromTag() {
  // In CI a tag push sets GITHUB_REF_NAME (e.g. "v0.2.3"); locally fall back to git.
  const ref = process.env.GITHUB_REF_NAME || git(["describe", "--tags", "--abbrev=0"]);
  return ref;
}

// ---- parse args -------------------------------------------------------------
const args = process.argv.slice(2);
const flags = new Set(args.filter((a) => a.startsWith("--")));
const positional = args.find((a) => !a.startsWith("--"));

if (flags.has("--check")) {
  const versions = {
    "package.json": readJsonVersion(PKG),
    "tauri.conf.json": readJsonVersion(CONF),
    "Cargo.toml": readCargoVersion(),
    "Cargo.lock": readCargoLockVersion(),
  };
  const unique = [...new Set(Object.values(versions))];
  if (unique.length === 1) {
    console.log(`set-version: all manifests agree on ${unique[0]}`);
    process.exit(0);
  }
  console.error("set-version: version mismatch across manifests:");
  for (const [file, v] of Object.entries(versions)) console.error(`  ${file}: ${v}`);
  console.error("Run `npm run set-version <version>` to sync them.");
  process.exit(1);
}

const version = normalize(flags.has("--from-tag") ? fromTag() : positional ?? die("no version given (pass X.Y.Z or --from-tag)"));

writeJsonVersion(PKG, version);
writeJsonVersion(CONF, version);
writeCargoVersion(version);
writeCargoLockVersion(version);
console.log(`set-version: set ${version} in package.json, tauri.conf.json, Cargo.toml, Cargo.lock`);

if (flags.has("--commit")) {
  git(["add", "package.json", "src-tauri/tauri.conf.json", "src-tauri/Cargo.toml", "Cargo.lock"]);
  git(["commit", "-m", `chore: release v${version}`]);
  console.log(`set-version: committed "chore: release v${version}"`);
}

if (flags.has("--tag")) {
  git(["tag", "-a", `v${version}`, "-m", `v${version}`]);
  console.log(`set-version: created tag v${version}`);
}
