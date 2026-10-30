import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { test } from "node:test";

import { assetUrl, clientTarget, pinnedDigest, verify } from "./client.ts";

test("clients exist for Linux on x64 and arm64", () => {
  assert.equal(clientTarget("linux", "x64"), "x86_64-unknown-linux-musl");
  assert.equal(clientTarget("linux", "arm64"), "aarch64-unknown-linux-musl");
  assert.equal(clientTarget("darwin", "arm64"), undefined);
  assert.equal(clientTarget("linux", "riscv64"), undefined);
});

test("asset URLs point at the client release", () => {
  assert.equal(
    assetUrl("x86_64-unknown-linux-musl", "1.2.3"),
    "https://codeberg.org/inomotech/calf-granary/releases/download/client-v1.2.3/calf-granary-client-x86_64-unknown-linux-musl",
  );
});

test("an empty or missing digest is refused", () => {
  const hash = "a".repeat(64);
  assert.equal(pinnedDigest("x", { x: hash }), hash);
  assert.throws(() => pinnedDigest("x", { x: "" }), /pins no client digest/);
  assert.throws(() => pinnedDigest("x", {}), /pins no client digest for x/);
});

test("verify rejects data with another hash", () => {
  const data = new TextEncoder().encode("client");
  const hash = createHash("sha256").update(data).digest("hex");
  verify(data, hash);
  assert.throws(() => verify(data, "0".repeat(64)), /expected 0{64}/);
});
