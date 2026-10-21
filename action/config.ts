// Configuration that points Nix and BitBake at granary.

import type { CacheInfo } from "./granary.ts";

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

/** A netrc file that sends the token to the server. */
export function netrc(server: string, token: string): string {
  return `machine ${new URL(server).hostname}\nlogin granary\npassword ${token}\n`;
}

/** BitBake settings for the caches, reporting hash equivalence only with a
 * token. */
export function bitbakeConfig(caches: readonly CacheInfo[], token?: string): string {
  const lines = ["# Written by the calf-granary action."];
  const sstate = caches.find((cache) => cache.protocol === "files" && cache.role === "sstate");
  if (sstate !== undefined) {
    lines.push(`SSTATE_MIRRORS ?= "file://.* ${sstate.url}/PATH;downloadfilename=PATH"`);
  }
  const downloads = caches.find(
    (cache) => cache.protocol === "files" && cache.role === "downloads",
  );
  if (downloads !== undefined) {
    const schemes = ["git", "gitsm", "https?", "ftp", "npm", "crate"];
    const mirrors = schemes.map((scheme) => `    ${scheme}://.*/.* ${downloads.url}/ \\n \\`);
    lines.push(`PREMIRRORS:prepend = "\\`, ...mirrors, `"`);
    lines.push(`BB_GENERATE_MIRROR_TARBALLS = "1"`);
  }
  const hashserv = caches.find((cache) => cache.protocol === "hashserv");
  if (hashserv !== undefined) {
    lines.push(`BB_SIGNATURE_HANDLER = "OEEquivHash"`, `BB_HASHSERVE = "${hashserv.url}"`);
    if (token !== undefined) {
      lines.push(`BB_HASHSERVE_USERNAME = "granary"`, `BB_HASHSERVE_PASSWORD = "${token}"`);
    }
  }
  return `${lines.join("\n")}\n`;
}
