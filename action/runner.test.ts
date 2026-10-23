import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import {
  booleanInput,
  input,
  requiredInput,
  writeFileCommand,
} from "./runner.ts";

test("inputs come from INPUT_ variables", () => {
  const env = { "INPUT_BITBAKE-CONF": " conf/auto.conf ", INPUT_NIX: "True" };
  assert.equal(input("bitbake-conf", env), "conf/auto.conf");
  assert.equal(booleanInput("nix", env), true);
  assert.equal(input("missing", env), "");
  assert.throws(
    () => requiredInput("missing", env),
    /the input missing is required/,
  );
});

test("file commands survive multi-line values", () => {
  const file = join(mkdtempSync(join(tmpdir(), "granary-")), "env");
  writeFileSync(file, "");
  writeFileCommand("GITHUB_ENV", "NIX_CONFIG", "a = 1\nb = 2", {
    GITHUB_ENV: file,
  });
  const [header, first, second, footer] = readFileSync(file, "utf8").split(
    "\n",
  );
  const delimiter = header?.split("<<")[1];
  assert.match(header ?? "", /^NIX_CONFIG<<granary_/);
  assert.deepEqual([first, second], ["a = 1", "b = 2"]);
  assert.equal(footer, delimiter);
});
