import type { SessionStorage } from "./session";

const entryKey = "selfless.invite.idempotency";

function randomUuid() {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = Array.from(bytes, (x) => x.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

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
