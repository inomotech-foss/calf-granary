import assert from "node:assert/strict";
import { test } from "node:test";

import { bitbakeConfig, netrc, nixConfig, replaceBlock } from "./config.ts";
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
  const config = bitbakeConfig(caches);
  assert.match(config, /^SSTATE_MIRRORS \?= "file:\/\/\.\* https:\/\/g\.example\/sstate-pr\/PATH;downloadfilename=PATH"$/m);
  assert.match(config, /^ {4}git:\/\/\.\*\/\.\* https:\/\/g\.example\/downloads\/ \\n \\$/m);
  assert.match(config, /^BB_GENERATE_MIRROR_TARBALLS = "1"$/m);
  assert.match(config, /^BB_HASHSERVE = "wss:\/\/g\.example\/_hashserv\/hash-pr"$/m);
  assert.doesNotMatch(config, /PASSWORD/);
});

test("the settings replace the ones from earlier runs", () => {
  const first = replaceBlock("BB_NUMBER_THREADS = \"4\"", "A = \"1\"\n");
  const second = replaceBlock(first, "A = \"2\"\n");
  assert.equal(
    second,
    'BB_NUMBER_THREADS = "4"\n# Begin calf-granary settings\nA = "2"\n# End calf-granary settings\n',
  );
  assert.equal(replaceBlock(second), 'BB_NUMBER_THREADS = "4"\n');
});

test("BitBake settings only cover granted caches", () => {
  const config = bitbakeConfig([caches[0]!]);
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
    netrc(["g.example", "wss://g.example/_hashserv/hash-pr"], "secret"),
    "machine g.example\nlogin granary\npassword secret\n" +
      "machine wss://g.example/_hashserv/hash-pr\nlogin granary\npassword secret\n",
  );
});

test("anonymous jobs read without credentials", () => {
  assert.doesNotMatch(nixConfig(caches[3]!), /netrc-file/);
});
