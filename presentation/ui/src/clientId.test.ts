import { clientId } from "./clientId";
import type { SessionStorage } from "./session";

function fakeStorage(items = new Map<string, string>()): SessionStorage {
  return {
    getItem: (key) => items.get(key) ?? null,
    setItem: (key, value) => void items.set(key, value),
    removeItem: (key) => void items.delete(key),
  };
}

test("the client id is the one in storage", () => {
  const storage = fakeStorage(new Map([["selfless.client.id", "0f8e3c2a-5b7d-4e19-9a64-2c1d8b7f6e30"]]));

  expect(clientId(storage)).toBe("0f8e3c2a-5b7d-4e19-9a64-2c1d8b7f6e30");
});

test("without a client id in storage, a new UUID is made once and kept", () => {
  const storage = fakeStorage();

  const made = clientId(storage);

  expect(made).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
  expect(storage.getItem("selfless.client.id")).toBe(made);
  expect(clientId(storage)).toBe(made);
});
