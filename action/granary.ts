// Getting a granary token for the job.

/** A cache that a granary token grants. */
export interface CacheInfo {
  readonly name: string;
  readonly protocol: "files" | "nix" | "hashserv";
  readonly role: string;
  readonly url: string;
  readonly access: "write" | "read";
  readonly public_key?: string;
}

/** A granary token and the caches it grants. */
export interface Issued {
  readonly token: string;
  readonly expires_at: number;
  readonly tier: string;
  readonly caches: readonly CacheInfo[];
}

/** Where the forge hands out ID tokens to the job. */
export interface IdTokenSource {
  readonly ACTIONS_ID_TOKEN_REQUEST_URL?: string | undefined;
  readonly ACTIONS_ID_TOKEN_REQUEST_TOKEN?: string | undefined;
}

/** Requests an ID token from the forge for `audience`. */
export async function requestIdToken(
  audience: string,
  source: IdTokenSource = process.env,
): Promise<string> {
  const url = source.ACTIONS_ID_TOKEN_REQUEST_URL;
  const token = source.ACTIONS_ID_TOKEN_REQUEST_TOKEN;
  if (url === undefined || token === undefined || url === "" || token === "") {
    throw new Error("the job can't request an ID token, grant it `id-token: write`");
  }
  const request = new URL(url);
  request.searchParams.set("audience", audience);
  const response = await fetch(request, { headers: { authorization: `Bearer ${token}` } });
  if (!response.ok) {
    throw new Error(`the forge refused an ID token with ${response.status}`);
  }
  const body: unknown = await response.json();
  if (!isRecord(body) || typeof body["value"] !== "string") {
    throw new Error("the forge answered without an ID token");
  }
  return body["value"];
}

/** Exchanges an ID token for a granary token. */
export async function exchange(server: string, idToken: string): Promise<Issued> {
  const response = await fetch(new URL("token", withSlash(server)), {
    method: "POST",
    headers: { authorization: `Bearer ${idToken}` },
  });
  if (!response.ok) {
    throw new Error(`granary refused the ID token with ${response.status}`);
  }
  const body: unknown = await response.json();
  if (!isIssued(body)) {
    throw new Error("granary answered with an unknown format");
  }
  return body;
}

/** Lists the caches that anyone may read, for jobs without an ID token. */
export async function publicCaches(server: string): Promise<CacheInfo[]> {
  const response = await fetch(new URL("_api/caches", withSlash(server)));
  if (!response.ok) {
    throw new Error(`granary refused to list its caches with ${response.status}`);
  }
  const body: unknown = await response.json();
  if (!isRecord(body) || !Array.isArray(body["caches"]) || !body["caches"].every(isCacheInfo)) {
    throw new Error("granary answered with an unknown format");
  }
  return body["caches"];
}

/** Ensures a base URL ends with a slash, so relative URLs extend it. */
function withSlash(url: string): string {
  return url.endsWith("/") ? url : `${url}/`;
}

/** Checks the shape of a token response. */
function isIssued(value: unknown): value is Issued {
  return (
    isRecord(value) &&
    typeof value["token"] === "string" &&
    typeof value["expires_at"] === "number" &&
    typeof value["tier"] === "string" &&
    Array.isArray(value["caches"]) &&
    value["caches"].every(isCacheInfo)
  );
}

/** Checks the shape of a cache description. */
function isCacheInfo(value: unknown): value is CacheInfo {
  return (
    isRecord(value) &&
    typeof value["name"] === "string" &&
    ["files", "nix", "hashserv"].includes(String(value["protocol"])) &&
    typeof value["role"] === "string" &&
    typeof value["url"] === "string" &&
    ["write", "read"].includes(String(value["access"])) &&
    (value["public_key"] === undefined || typeof value["public_key"] === "string")
  );
}

/** Checks that a value is a plain object. */
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
