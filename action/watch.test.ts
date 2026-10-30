import assert from "node:assert/strict";
import { test } from "node:test";

import type { CacheInfo } from "./granary.ts";
import { watchArgs } from "./watch.ts";

const caches: CacheInfo[] = [
  {
    name: "sstate-pr",
    protocol: "files",
    role: "sstate",
    url: "https://g.example/sstate-pr",
    access: "write",
  },
  {
    name: "downloads",
    protocol: "files",
    role: "downloads",
    url: "https://g.example/downloads",
    access: "read",
  },
  {
    name: "nix-pr",
    protocol: "nix",
    role: "nix",
    url: "https://g.example/nix-pr",
    access: "write",
  },
];

const sources = {
  knownPaths: "/tmp/paths",
  sstateDir: "/build/sstate",
  downloadsDir: "/build/downloads",
  since: "1700000000",
  interval: "120",
};

test("the uploader only covers writable caches", () => {
  assert.deepEqual(watchArgs(caches, sources), [
    "watch",
    "--interval",
    "120",
    "--since",
    "1700000000",
    "--nix-cache",
    "nix-pr",
    "--known-paths",
    "/tmp/paths",
    "--sstate-cache",
    "sstate-pr",
    "--sstate-dir",
    "/build/sstate",
  ]);
});

test("the uploader doesn't start without anything to upload", () => {
  assert.equal(
    watchArgs(caches, { ...sources, knownPaths: "", sstateDir: "" }),
    undefined,
  );
});
