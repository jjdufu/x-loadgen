import { isAxiosError } from "axios";
import { http } from "../api/http";
import type { MeResponse } from "../api/types";

let cachedMe: MeResponse | null = null;
let inflightMe: Promise<MeResponse> | null = null;

export function setCachedMe(me: MeResponse | null): void {
  cachedMe = me;
}

export function invalidateCachedMe(): void {
  cachedMe = null;
}

/**
 * Return cached `/api/v1/auth/me` state, deduplicating concurrent probes.
 *
 * Only explicit authentication failures (401/403) evict an already-verified
 * session; transient network/5xx blips during route navigation keep the last
 * known state instead of kicking an authenticated user back to `/login`.
 */
export async function getCachedMe(force = false): Promise<MeResponse> {
  if (!force && cachedMe) {
    return cachedMe;
  }
  if (!force && inflightMe) {
    return inflightMe;
  }
  const req = http
    .get<MeResponse>("/api/v1/auth/me", { silent: true })
    .then(({ data }) => {
      cachedMe = data;
      return data;
    })
    .catch((err: unknown) => {
      const status = isAxiosError(err) ? err.response?.status : undefined;
      if (status === 401 || status === 403) {
        cachedMe = null;
        throw err;
      }
      if (cachedMe) {
        return cachedMe;
      }
      throw err;
    })
    .finally(() => {
      if (inflightMe === req) {
        inflightMe = null;
      }
    });
  inflightMe = req;
  return req;
}
