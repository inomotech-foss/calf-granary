import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { test } from "node:test";

import { assetName, assetUrl, parseChecksum, verify } from "./client.ts";

test("assets exist for Linux on x64 and arm64", () => {
  assert.equal(
    assetName("linux", "x64"),
    "calf-granary-client-x86_64-unknown-linux-musl",
  );
  assert.equal(
    assetName("linux", "arm64"),
    "calf-granary-client-aarch64-unknown-linux-musl",
  );
  assert.equal(assetName("darwin", "arm64"), undefined);
  assert.equal(assetName("linux", "riscv64"), undefined);
});

test("asset URLs point at the client release", () => {
  assert.equal(
    assetUrl("calf-granary-client-x86_64-unknown-linux-musl", "1.2.3"),
    "https://codeberg.org/inomotech/calf-granary/releases/download/client-v1.2.3/calf-granary-client-x86_64-unknown-linux-musl",
  );
});

test("checksums come from sha256sum lines", () => {
  const hash = "a".repeat(64);
  assert.equal(parseChecksum(`${hash}  calf-granary-client\n`), hash);
  assert.equal(parseChecksum(hash.toUpperCase()), hash);
  assert.throws(() => parseChecksum("<html>"), /unknown format/);
});

test("verify rejects data with another hash", () => {
  const data = new TextEncoder().encode("client");
  const hash = createHash("sha256").update(data).digest("hex");
  verify(data, hash);
  assert.throws(() => verify(data, "0".repeat(64)), /expected 0{64}/);
});
