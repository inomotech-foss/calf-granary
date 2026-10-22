// Gets a granary token and points Nix and BitBake at the caches.

import { chmodSync, existsSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

import { bitbakeConfig, netrc, nixConfig, replaceBlock } from "./config.ts";
import { type CacheInfo, exchange, type Issued, publicCaches, requestIdToken } from "./granary.ts";
import { output } from "./process.ts";
import {
  booleanInput,
  exportVariable,
  fail,
  info,
  input,
  requiredInput,
  saveState,
  setOutput,
  setSecret,
  warning,
} from "./runner.ts";

/** Gets a granary token, or warns and returns nothing. */
async function obtainToken(server: string): Promise<Issued | undefined> {
  try {
    const idToken = await requestIdToken(input("audience") || "calf-granary");
    setSecret(idToken);
    const issued = await exchange(server, idToken);
    setSecret(issued.token);
    return issued;
  } catch (err) {
    warning(
      `calf-granary: ${err instanceof Error ? err.message : String(err)}, ` +
        "reading the public caches without uploading",
    );
    return undefined;
  }
}

/** Sets up the caches. */
async function main(): Promise<void> {
  const started = Math.floor(Date.now() / 1000);
  const server = requiredInput("server");
  const issued = await obtainToken(server);
  const caches = writableFirst(issued?.caches ?? (await publicCaches(server)));
  if (issued !== undefined) {
    saveState("token", issued.token);
    saveState("started", String(started));
    saveState("caches", JSON.stringify(issued.caches));
    setOutput("tier", issued.tier);
  }
  info(`granary tier ${issued?.tier ?? "anonymous"}: ${caches.map((cache) => cache.name).join(", ")}`);

  const temp = process.env["RUNNER_TEMP"] ?? "/tmp";
  let netrcFile: string | undefined;
  if (issued !== undefined) {
    netrcFile = join(temp, "granary.netrc");
    writeFileSync(netrcFile, netrc([new URL(server).hostname], issued.token), { mode: 0o600 });
    saveState("netrc", netrcFile);
  }
  const nixCache = caches.find((cache) => cache.protocol === "nix");
  if (booleanInput("nix") && nixCache !== undefined) {
    if (issued !== undefined) {
      const snapshot = join(temp, "granary-store-paths");
      writeFileSync(snapshot, await output("nix", ["path-info", "--all"]));
      saveState("store-paths", snapshot);
    }
    const existing = process.env["NIX_CONFIG"] ?? "";
    exportVariable("NIX_CONFIG", `${existing}\n${nixConfig(nixCache, netrcFile)}`.trim());
    info(`substituting from ${nixCache.url}`);
  }

  const bitbakeFile = input("bitbake-conf");
  if (bitbakeFile !== "") {
    writeFileSync(bitbakeFile, replaceBlock(readIfExists(bitbakeFile), bitbakeConfig(caches)));
    info(`wrote the BitBake settings to ${bitbakeFile}`);
    if (issued !== undefined) {
      const hashserv = caches.filter((cache) => cache.protocol === "hashserv").map((cache) => cache.url);
      const machines = [new URL(server).hostname, ...hashserv];
      const homeNetrc = join(homedir(), ".netrc");
      writeFileSync(homeNetrc, replaceBlock(readIfExists(homeNetrc), netrc(machines, issued.token)), {
        mode: 0o600,
      });
      chmodSync(homeNetrc, 0o600);
      saveState("home-netrc", homeNetrc);
    }
  }
}

/** Orders caches so that BitBake and Nix use the ones the job writes to. */
function writableFirst(caches: readonly CacheInfo[]): CacheInfo[] {
  return [...caches].sort((a, b) => Number(b.access === "write") - Number(a.access === "write"));
}

/** Reads a file, or returns an empty string if there is none. */
function readIfExists(file: string): string {
  return existsSync(file) ? readFileSync(file, "utf8") : "";
}

main().catch((err: unknown) => {
  fail(`calf-granary: ${err instanceof Error ? err.message : String(err)}`);
});
