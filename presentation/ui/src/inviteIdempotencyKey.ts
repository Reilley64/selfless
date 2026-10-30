import { randomUuid } from "./randomUuid";
import type { SessionStorage } from "./session";

const entryKey = "selfless.invite.idempotency";

export function inviteIdempotencyKey(storage: SessionStorage, token: string): string {
  const stored = JSON.parse(storage.getItem(entryKey) ?? "null") as { token: string; key: string } | null;
  if (stored?.token === token) {
    return stored.key;
  }
  const key = randomUuid();
  storage.setItem(entryKey, JSON.stringify({ token, key }));
  return key;
}

export function forgetInviteIdempotencyKey(storage: SessionStorage): void {
  storage.removeItem(entryKey);
}
