// Gets a granary token and points Nix and BitBake at the caches.

import { appendFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { bitbakeConfig, netrc, nixConfig } from "./config.ts";
import { exchange, requestIdToken } from "./granary.ts";
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
} from "./runner.ts";

/** Sets up the caches. */
async function main(): Promise<void> {
  const started = Math.floor(Date.now() / 1000);
  const server = requiredInput("server");
  const idToken = await requestIdToken(input("audience") || "calf-granary");
  setSecret(idToken);
  const issued = await exchange(server, idToken);
  setSecret(issued.token);
  saveState("token", issued.token);
  saveState("started", String(started));
  saveState("caches", JSON.stringify(issued.caches));
  setOutput("tier", issued.tier);
  info(`granary tier ${issued.tier}: ${issued.caches.map((cache) => cache.name).join(", ")}`);

  const temp = process.env["RUNNER_TEMP"] ?? "/tmp";
  const nixCache = issued.caches.find((cache) => cache.protocol === "nix");
  if (booleanInput("nix") && nixCache !== undefined) {
    const netrcFile = join(temp, "granary.netrc");
    writeFileSync(netrcFile, netrc(server, issued.token), { mode: 0o600 });
    const existing = process.env["NIX_CONFIG"] ?? "";
    exportVariable("NIX_CONFIG", `${existing}\n${nixConfig(nixCache, netrcFile)}`.trim());
    const snapshot = join(temp, "granary-store-paths");
    writeFileSync(snapshot, await output("nix", ["path-info", "--all"]));
    saveState("store-paths", snapshot);
    info(`substituting from ${nixCache.url}`);
  }

  const bitbakeFile = input("bitbake-conf");
  if (bitbakeFile !== "") {
    appendFileSync(bitbakeFile, bitbakeConfig(issued.caches, issued.token), { mode: 0o600 });
    info(`wrote the BitBake settings to ${bitbakeFile}`);
  }
}

main().catch((err: unknown) => {
  fail(`calf-granary: ${err instanceof Error ? err.message : String(err)}`);
});
