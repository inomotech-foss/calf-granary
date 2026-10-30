// Gets a granary token and points Nix and BitBake at the caches.

import { chmodSync, existsSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

import { clientTarget, installClient } from "./client.ts";
import { bitbakeConfig, netrc, nixConfig, replaceBlock } from "./config.ts";
import {
  type CacheInfo,
  exchange,
  type Issued,
  publicCaches,
  requestIdToken,
} from "./granary.ts";
import { detach, output } from "./process.ts";
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
import { type Sources, watchArgs } from "./watch.ts";

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

/** A granary token and the client that uploads with it. */
interface Uploader {
  readonly issued: Issued;
  readonly client: string;
}

/** Gets a token and the client if the job can upload, or warns and returns
 * nothing. */
async function obtainUploader(server: string): Promise<Uploader | undefined> {
  const client = input("client");
  if (client !== "") {
    const issued = await obtainToken(server);
    return issued === undefined ? undefined : { issued, client };
  }
  const target = clientTarget(process.platform, process.arch);
  if (target === undefined) {
    warning(
      `calf-granary: there is no client for ${process.platform} ${process.arch}, ` +
        "reading the public caches without uploading",
    );
    return undefined;
  }
  const issued = await obtainToken(server);
  if (issued === undefined) {
    return undefined;
  }
  const cacheDir =
    process.env["RUNNER_TOOL_CACHE"] || process.env["RUNNER_TEMP"] || "/tmp";
  const installed = await installClient(target, cacheDir);
  saveState("client", installed);
  return { issued, client: installed };
}

/** Sets up the caches. */
async function main(): Promise<void> {
  const started = Math.floor(Date.now() / 1000);
  const server = requiredInput("server");
  const uploader = await obtainUploader(server);
  const issued = uploader?.issued;
  const caches = writableFirst(issued?.caches ?? (await publicCaches(server)));
  if (issued !== undefined) {
    saveState("token", issued.token);
    saveState("started", String(started));
    saveState("caches", JSON.stringify(issued.caches));
    setOutput("tier", issued.tier);
  }
  info(
    `granary tier ${issued?.tier ?? "anonymous"}: ${caches.map((cache) => cache.name).join(", ")}`,
  );

  const temp = process.env["RUNNER_TEMP"] ?? "/tmp";
  let netrcFile: string | undefined;
  if (issued !== undefined) {
    netrcFile = join(temp, "granary.netrc");
    writeFileSync(netrcFile, netrc([new URL(server).hostname], issued.token), {
      mode: 0o600,
    });
    saveState("netrc", netrcFile);
  }
  const nixCache = caches.find((cache) => cache.protocol === "nix");
  let snapshot = "";
  if (booleanInput("nix") && nixCache !== undefined) {
    if (issued !== undefined) {
      snapshot = join(temp, "granary-store-paths");
      writeFileSync(snapshot, await output("nix", ["path-info", "--all"]));
      saveState("store-paths", snapshot);
    }
    const existing = process.env["NIX_CONFIG"] ?? "";
    exportVariable(
      "NIX_CONFIG",
      `${existing}\n${nixConfig(nixCache, netrcFile)}`.trim(),
    );
    info(`substituting from ${nixCache.url}`);
  }

  const bitbakeFile = input("bitbake-conf");
  if (bitbakeFile !== "") {
    writeFileSync(
      bitbakeFile,
      replaceBlock(readIfExists(bitbakeFile), bitbakeConfig(caches)),
    );
    info(`wrote the BitBake settings to ${bitbakeFile}`);
    if (issued !== undefined) {
      const hashserv = caches
        .filter((cache) => cache.protocol === "hashserv")
        .map((cache) => cache.url);
      const machines = [new URL(server).hostname, ...hashserv];
      const homeNetrc = join(homedir(), ".netrc");
      writeFileSync(
        homeNetrc,
        replaceBlock(readIfExists(homeNetrc), netrc(machines, issued.token)),
        {
          mode: 0o600,
        },
      );
      chmodSync(homeNetrc, 0o600);
      saveState("home-netrc", homeNetrc);
    }
  }

  const interval = input("upload-interval") || "120";
  if (uploader !== undefined && interval !== "0") {
    startWatcher(server, uploader, join(temp, "granary-watch.log"), {
      knownPaths: snapshot,
      sstateDir: input("sstate-dir"),
      downloadsDir: input("downloads-dir"),
      since: String(started),
      interval,
    });
  }
}

/** Starts uploading what the job builds while it runs. */
function startWatcher(
  server: string,
  { issued, client }: Uploader,
  log: string,
  sources: Sources,
): void {
  const args = watchArgs(issued.caches, sources);
  if (args === undefined) {
    return;
  }
  const pid = detach(
    client,
    args,
    { ...process.env, GRANARY_SERVER: server, GRANARY_TOKEN: issued.token },
    log,
  );
  if (pid === undefined) {
    warning("calf-granary: the uploader didn't start, uploading after the job");
    return;
  }
  saveState("watcher", String(pid));
  saveState("watch-log", log);
  info(`uploading every ${sources.interval} seconds while the job runs`);
}

/** Orders caches so that BitBake and Nix use the ones the job writes to. */
function writableFirst(caches: readonly CacheInfo[]): CacheInfo[] {
  return [...caches].sort(
    (a, b) => Number(b.access === "write") - Number(a.access === "write"),
  );
}

/** Reads a file, or returns an empty string if there is none. */
function readIfExists(file: string): string {
  return existsSync(file) ? readFileSync(file, "utf8") : "";
}

main().catch((err: unknown) => {
  fail(`calf-granary: ${err instanceof Error ? err.message : String(err)}`);
});
