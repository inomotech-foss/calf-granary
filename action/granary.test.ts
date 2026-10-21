import assert from "node:assert/strict";
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";
import { test } from "node:test";

import { exchange, requestIdToken } from "./granary.ts";

/** Serves `handler` on a free port and returns its base URL. */
async function serve(
  handler: (request: IncomingMessage, response: ServerResponse) => void,
): Promise<{ url: string; close: () => void }> {
  const server = createServer(handler);
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address() as AddressInfo;
  return { url: `http://127.0.0.1:${String(port)}`, close: () => server.close() };
}

test("ID tokens are requested for the audience", async () => {
  const forge = await serve((request, response) => {
    const url = new URL(request.url ?? "", "http://forge");
    assert.equal(request.headers.authorization, "Bearer runner-token");
    assert.equal(url.searchParams.get("audience"), "calf-granary");
    assert.equal(url.searchParams.get("api-version"), "2.0");
    response.end(JSON.stringify({ value: "id-token" }));
  });
  try {
    const token = await requestIdToken("calf-granary", {
      ACTIONS_ID_TOKEN_REQUEST_URL: `${forge.url}/token?api-version=2.0`,
      ACTIONS_ID_TOKEN_REQUEST_TOKEN: "runner-token",
    });
    assert.equal(token, "id-token");
  } finally {
    forge.close();
  }
});

test("jobs without ID tokens get a clear error", async () => {
  await assert.rejects(requestIdToken("calf-granary", {}), /id-token: write/);
});

test("ID tokens are exchanged at /token", async () => {
  const granary = await serve((request, response) => {
    assert.equal(request.method, "POST");
    assert.equal(request.url, "/base/token");
    assert.equal(request.headers.authorization, "Bearer id-token");
    response.end(
      JSON.stringify({
        token: "granary-token",
        expires_at: 1,
        tier: "pr",
        caches: [{ name: "sstate-pr", protocol: "files", role: "sstate", url: "u" }],
      }),
    );
  });
  try {
    const issued = await exchange(`${granary.url}/base`, "id-token");
    assert.equal(issued.token, "granary-token");
    assert.equal(issued.caches[0]?.role, "sstate");
  } finally {
    granary.close();
  }
});

test("refused exchanges fail", async () => {
  const granary = await serve((_request, response) => {
    response.statusCode = 403;
    response.end();
  });
  try {
    await assert.rejects(exchange(granary.url, "id-token"), /403/);
  } finally {
    granary.close();
  }
});
