import assert from "node:assert/strict";
import { test } from "node:test";

import { bitbakeConfig, netrc, nixConfig } from "./config.ts";
import type { CacheInfo } from "./granary.ts";

const caches: CacheInfo[] = [
  { name: "sstate-pr", protocol: "files", role: "sstate", url: "https://g.example/sstate-pr" },
  { name: "downloads", protocol: "files", role: "downloads", url: "https://g.example/downloads" },
  { name: "hash-pr", protocol: "hashserv", role: "hashserv", url: "wss://g.example/_hashserv/hash-pr" },
  {
    name: "nix-pr",
    protocol: "nix",
    role: "nix",
    url: "https://g.example/nix-pr",
    public_key: "nix-pr-1:AAAA",
  },
];

test("BitBake reads sstate, downloads and hash equivalence from granary", () => {
  const config = bitbakeConfig(caches, "secret");
  assert.match(config, /^SSTATE_MIRRORS \?= "file:\/\/\.\* https:\/\/g\.example\/sstate-pr\/PATH;downloadfilename=PATH"$/m);
  assert.match(config, /^ {4}git:\/\/\.\*\/\.\* https:\/\/g\.example\/downloads\/ \\n \\$/m);
  assert.match(config, /^BB_GENERATE_MIRROR_TARBALLS = "1"$/m);
  assert.match(config, /^BB_HASHSERVE = "wss:\/\/g\.example\/_hashserv\/hash-pr"$/m);
  assert.match(config, /^BB_HASHSERVE_PASSWORD = "secret"$/m);
});

test("BitBake settings only cover granted caches", () => {
  const config = bitbakeConfig([caches[0]!], "secret");
  assert.match(config, /SSTATE_MIRRORS/);
  assert.doesNotMatch(config, /PREMIRRORS|BB_HASHSERVE/);
});

test("Nix substitutes with the cache's key and the token", () => {
  assert.equal(
    nixConfig(caches[3]!, "/tmp/granary.netrc"),
    "extra-substituters = https://g.example/nix-pr\n" +
      "netrc-file = /tmp/granary.netrc\n" +
      "extra-trusted-public-keys = nix-pr-1:AAAA\n",
  );
  assert.equal(
    netrc("https://g.example:8443/", "secret"),
    "machine g.example\nlogin granary\npassword secret\n",
  );
});

test("anonymous jobs read without credentials", () => {
  const config = bitbakeConfig(caches);
  assert.match(config, /^BB_HASHSERVE = "wss:/m);
  assert.doesNotMatch(config, /BB_HASHSERVE_PASSWORD/);
  assert.doesNotMatch(nixConfig(caches[3]!), /netrc-file/);
});
