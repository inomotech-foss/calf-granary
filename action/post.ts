// Uploads what the job built.

import { readFileSync, rmSync, writeFileSync } from "node:fs";

import { replaceBlock } from "./config.ts";
import type { CacheInfo } from "./granary.ts";
import { output, run } from "./process.ts";
import { info, input, state, warning } from "./runner.ts";

/** Uploads new Nix store paths and Yocto files. */
async function post(): Promise<void> {
  const token = state("token");
  if (token === "") {
    info("granary wasn't set up, nothing to upload");
    return;
  }
  const caches = (JSON.parse(state("caches")) as CacheInfo[]).filter((cache) => cache.access === "write");
  const client = input("client") || "calf-granary-client";
  const env = { ...process.env, GRANARY_SERVER: input("server"), GRANARY_TOKEN: token };

  const snapshot = state("store-paths");
  const nixCache = caches.find((cache) => cache.protocol === "nix");
  if (snapshot !== "" && nixCache !== undefined) {
    const before = new Set(readFileSync(snapshot, "utf8").split("\n"));
    const paths = (await output("nix", ["path-info", "--all"]))
      .split("\n")
      .filter((path) => path !== "" && !before.has(path) && !path.endsWith(".drv"));
    if (paths.length > 0) {
      await upload(client, ["nix", "push", "--cache", nixCache.name, ...paths], env);
    }
  }

  const since = state("started");
  for (const [command, role, dir] of [
    ["push-sstate", "sstate", input("sstate-dir")],
    ["push-downloads", "downloads", input("downloads-dir")],
  ] as const) {
    const cache = caches.find((candidate) => candidate.protocol === "files" && candidate.role === role);
    if (dir !== "" && cache !== undefined) {
      await upload(client, ["yocto", command, "--cache", cache.name, "--dir", dir, "--since", since], env);
    }
  }
}

/** Removes the credentials that main wrote. */
function removeCredentials(): void {
  const netrcFile = state("netrc");
  if (netrcFile !== "") {
    rmSync(netrcFile, { force: true });
  }
  const homeNetrc = state("home-netrc");
  if (homeNetrc !== "") {
    const rest = replaceBlock(readFileSync(homeNetrc, "utf8"));
    if (rest.trim() === "") {
      rmSync(homeNetrc, { force: true });
    } else {
      writeFileSync(homeNetrc, rest);
    }
  }
}

/** Runs an upload, warning instead of failing the job. */
async function upload(client: string, args: string[], env: NodeJS.ProcessEnv): Promise<void> {
  try {
    await run(client, args, env);
  } catch (err) {
    warning(`calf-granary: ${args.slice(0, 2).join(" ")} failed: ${err instanceof Error ? err.message : String(err)}`);
  }
}

post()
  .finally(removeCredentials)
  .catch((err: unknown) => {
    warning(`calf-granary: ${err instanceof Error ? err.message : String(err)}`);
  });
