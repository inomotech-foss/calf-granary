// The uploader that runs while the job builds.

import { setTimeout as sleep } from "node:timers/promises";

import type { CacheInfo } from "./granary.ts";

/** What the uploader looks for new files in. */
export interface Sources {
  /** A file of the store paths before the job, or empty to skip Nix. */
  readonly knownPaths: string;
  readonly sstateDir: string;
  readonly downloadsDir: string;
  /** The Unix time that the job started. */
  readonly since: string;
  /** Seconds between scans. */
  readonly interval: string;
}

/** The client arguments that upload to the writable caches during the job,
 * or nothing if there is nothing to upload. */
export function watchArgs(
  caches: readonly CacheInfo[],
  sources: Sources,
): string[] | undefined {
  const writable = caches.filter((cache) => cache.access === "write");
  const args: string[] = [];
  const nixCache = writable.find((cache) => cache.protocol === "nix");
  if (sources.knownPaths !== "" && nixCache !== undefined) {
    args.push(
      "--nix-cache",
      nixCache.name,
      "--known-paths",
      sources.knownPaths,
    );
  }
  for (const [role, dir] of [
    ["sstate", sources.sstateDir],
    ["downloads", sources.downloadsDir],
  ] as const) {
    const cache = writable.find(
      (candidate) => candidate.protocol === "files" && candidate.role === role,
    );
    if (dir !== "" && cache !== undefined) {
      args.push(`--${role}-cache`, cache.name, `--${role}-dir`, dir);
    }
  }
  if (args.length === 0) {
    return undefined;
  }
  return [
    "watch",
    "--interval",
    sources.interval,
    "--since",
    sources.since,
    ...args,
  ];
}

/** Stops the uploader and waits a few seconds for it to exit. */
export async function stopWatcher(pid: number): Promise<void> {
  try {
    process.kill(pid, "SIGTERM");
  } catch {
    return;
  }
  for (let waited = 0; waited < 5000; waited += 100) {
    try {
      process.kill(pid, 0);
    } catch {
      return;
    }
    await sleep(100);
  }
}
