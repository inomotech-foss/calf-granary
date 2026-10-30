// Downloading the calf-granary-client release that matches the action.

import { createHash } from "node:crypto";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  renameSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";

import pinned from "./client.json" with { type: "json" };

const releases =
  "https://codeberg.org/inomotech/calf-granary/releases/download";

/** The release asset for a Node platform and architecture, if there is one. */
export function assetName(platform: string, arch: string): string | undefined {
  const targets: Record<string, string> = {
    x64: "x86_64-unknown-linux-musl",
    arm64: "aarch64-unknown-linux-musl",
  };
  const target = targets[arch];
  return platform === "linux" && target !== undefined
    ? `calf-granary-client-${target}`
    : undefined;
}

/** The download URL of a release asset of the pinned client. */
export function assetUrl(asset: string, version = pinned.version): string {
  return `${releases}/client-v${version}/${asset}`;
}

/** Reads the hash from a sha256sum line. */
export function parseChecksum(text: string): string {
  const hash = text.trim().split(/\s+/)[0]?.toLowerCase() ?? "";
  if (!/^[0-9a-f]{64}$/.test(hash)) {
    throw new Error("the client checksum has an unknown format");
  }
  return hash;
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
  asset: string,
  cacheDir: string,
): Promise<string> {
  const dir = join(cacheDir, "calf-granary-client", pinned.version);
  const program = join(dir, asset);
  if (existsSync(program)) {
    return program;
  }
  const expected = parseChecksum(
    new TextDecoder().decode(await download(assetUrl(`${asset}.sha256`))),
  );
  const data = await download(assetUrl(asset));
  verify(data, expected);
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
