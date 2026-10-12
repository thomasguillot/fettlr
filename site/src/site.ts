import { readFileSync } from "node:fs";
import { resolve } from "node:path";

export const SITE_URL = "https://fettlr.space.fast";
export const INSTALL_URL = `${SITE_URL}/install`;
export const REPO_URL = "https://github.com/thomasguillot/fettlr";
export const RELEASES_URL = `${REPO_URL}/releases/latest`;
export const LICENSE_URL = `${REPO_URL}/blob/main/LICENSE`;
export const AUTHOR_NAME = "Thomas Guillot";
export const AUTHOR_URL = "https://thomasguillot.com/";
export const year = String(new Date().getFullYear());

function readVersion(): string | undefined {
  try {
    const tauriConf = readFileSync(resolve(process.cwd(), "../app/src-tauri/tauri.conf.json"), "utf8");
    return JSON.parse(tauriConf).version;
  } catch {
    return undefined;
  }
}

export const version = readVersion();

/**
 * The install snippet points an agent that has shell access at this file and tells it to follow
 * whatever it says, so it names an immutable release tag rather than a branch: a reader runs what
 * that release shipped, not whatever `main` happens to hold the moment they paste it. The version
 * comes from the same `tauri.conf.json` the download link and the version badge read, so
 * `scripts/release.sh` moves all of them together. Refuses to build rather than falling back to a
 * branch, which would quietly put the unpinned URL back.
 */
export function bootstrapSkillUrl(): string {
  if (!version) {
    throw new Error(
      "Could not read the app version, so the install snippet cannot name a release tag. " +
        "Refusing to build rather than pointing the snippet at a branch.",
    );
  }
  const raw = REPO_URL.replace("//github.com/", "//raw.githubusercontent.com/");
  return `${raw}/v${version}/bootstrap/fettlr-launcher/SKILL.md`;
}
