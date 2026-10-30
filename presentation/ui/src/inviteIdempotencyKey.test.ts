import { forgetInviteIdempotencyKey, inviteIdempotencyKey } from "./inviteIdempotencyKey";
import type { SessionStorage } from "./session";

function fakeStorage(): SessionStorage {
  const items = new Map<string, string>();
  return {
    getItem: (key) => items.get(key) ?? null,
    setItem: (key, value) => void items.set(key, value),
    removeItem: (key) => void items.delete(key),
  };
}

const uuidV4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

test("the same invite token gets the same key after a reload", () => {
  const storage = fakeStorage();
  const key = inviteIdempotencyKey(storage, "a");
  expect(key).toMatch(uuidV4);
  expect(inviteIdempotencyKey(storage, "a")).toBe(key);
});

test("another invite token gets a new key and replaces the entry", () => {
  const storage = fakeStorage();
  const first = inviteIdempotencyKey(storage, "a");
  const second = inviteIdempotencyKey(storage, "b");
  expect(second).not.toBe(first);
  expect(inviteIdempotencyKey(storage, "b")).toBe(second);
  expect(inviteIdempotencyKey(storage, "a")).not.toBe(first);
});

test("after the key is forgotten the same token gets a new key", () => {
  const storage = fakeStorage();
  const first = inviteIdempotencyKey(storage, "a");
  forgetInviteIdempotencyKey(storage);
  expect(inviteIdempotencyKey(storage, "a")).not.toBe(first);
});
