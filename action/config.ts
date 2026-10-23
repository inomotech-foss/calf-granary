// Configuration that points Nix and BitBake at granary.

import type { CacheInfo } from "./granary.ts";

const BLOCK_START = "# Begin calf-granary settings";
const BLOCK_END = "# End calf-granary settings";

/** The settings that make Nix substitute from a cache, with the token in
 * `netrcFile` if there is one. */
export function nixConfig(cache: CacheInfo, netrcFile?: string): string {
  const lines = [`extra-substituters = ${cache.url}`];
  if (netrcFile !== undefined) {
    lines.push(`netrc-file = ${netrcFile}`);
  }
  if (cache.public_key !== undefined) {
    lines.push(`extra-trusted-public-keys = ${cache.public_key}`);
  }
  return `${lines.join("\n")}\n`;
}

/** Netrc entries that send the token to each machine. */
export function netrc(machines: readonly string[], token: string): string {
  return machines
    .map((machine) => `machine ${machine}\nlogin granary\npassword ${token}\n`)
    .join("");
}

/** Replaces the action's block in a configuration file's text, or removes it
 * without `block`. Both BitBake and netrc files take `#` comments. */
export function replaceBlock(text: string, block?: string): string {
  const start = text.indexOf(BLOCK_START);
  const end = text.indexOf(BLOCK_END, start);
  const rest =
    start === -1 || end === -1
      ? text
      : text.slice(0, start) +
        text.slice(end + BLOCK_END.length).replace(/^\n/, "");
  if (block === undefined) {
    return rest;
  }
  const separator = rest === "" || rest.endsWith("\n") ? "" : "\n";
  return `${rest}${separator}${BLOCK_START}\n${block}${BLOCK_END}\n`;
}

/** BitBake settings for the caches. Hash equivalence credentials come from
 * netrc, so they stay out of the build directory and the parse cache. */
export function bitbakeConfig(caches: readonly CacheInfo[]): string {
  const lines: string[] = [];
  const sstate = caches.find(
    (cache) => cache.protocol === "files" && cache.role === "sstate",
  );
  if (sstate !== undefined) {
    lines.push(
      `SSTATE_MIRRORS ?= "file://.* ${sstate.url}/PATH;downloadfilename=PATH"`,
    );
  }
  const downloads = caches.find(
    (cache) => cache.protocol === "files" && cache.role === "downloads",
  );
  if (downloads !== undefined) {
    const schemes = ["git", "gitsm", "https?", "ftp", "npm", "crate"];
    const mirrors = schemes.map(
      (scheme) => `    ${scheme}://.*/.* ${downloads.url}/ \\n \\`,
    );
    lines.push(`PREMIRRORS:prepend = "\\`, ...mirrors, `"`);
    lines.push(`BB_GENERATE_MIRROR_TARBALLS = "1"`);
  }
  const hashserv = caches.find((cache) => cache.protocol === "hashserv");
  if (hashserv !== undefined) {
    lines.push(
      `BB_SIGNATURE_HANDLER = "OEEquivHash"`,
      `BB_HASHSERVE = "${hashserv.url}"`,
    );
  }
  return `${lines.join("\n")}\n`;
}
