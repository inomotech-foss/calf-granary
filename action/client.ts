// Downloading the calf-granary-client release that matches the action.

import { createHash } from "node:crypto";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";

import pinned from "./client.json" with { type: "json" };

const releases =
  "https://codeberg.org/inomotech/calf-granary/releases/download";

/** The client target for a Node platform and architecture, if there is one. */
export function clientTarget(
  platform: string,
  arch: string,
): string | undefined {
  const targets: Record<string, string> = {
    x64: "x86_64-unknown-linux-musl",
    arm64: "aarch64-unknown-linux-musl",
  };
  return platform === "linux" ? targets[arch] : undefined;
}

/** The download URL of the pinned client for `target`. */
export function assetUrl(target: string, version = pinned.version): string {
  return `${releases}/client-v${version}/calf-granary-client-${target}`;
}

/** The SHA-256 hash that the action pins for `target`. */
export function pinnedDigest(
  target: string,
  digests: Readonly<Record<string, string>> = pinned.digests,
): string {
  const digest = digests[target] ?? "";
  if (digest === "") {
    throw new Error(`the action pins no client digest for ${target}`);
  }
  return digest;
}

/** Fails unless `data` has the SHA-256 hash `expected`. */
export function verify(data: Uint8Array, expected: string): void {
  const actual = createHash("sha256").update(data).digest("hex");
  if (actual !== expected) {
    throw new Error(
      `the client download has the hash ${actual}, expected ${expected}`,
    );
  }
}

/** Downloads and verifies the client into `cacheDir` and returns its path. */
export async function installClient(
  target: string,
  cacheDir: string,
): Promise<string> {
  const digest = pinnedDigest(target);
  const dir = join(cacheDir, "calf-granary-client", digest);
  const program = join(dir, "calf-granary-client");
  if (existsSync(program)) {
    verify(readFileSync(program), digest);
    return program;
  }
  const data = await download(assetUrl(target));
  verify(data, digest);
  mkdirSync(dir, { recursive: true });
  const partial = `${program}.${process.pid}`;
  writeFileSync(partial, data);
  chmodSync(partial, 0o755);
  renameSync(partial, program);
  return program;
}

/** Fetches a URL into memory. */
async function download(url: string): Promise<Uint8Array> {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`downloading ${url} failed with ${response.status}`);
  }
  return new Uint8Array(await response.arrayBuffer());
}
