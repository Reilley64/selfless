import { randomUuid } from "./randomUuid";
import type { SessionStorage } from "./session";

const clientIdKey = "selfless.client.id";

export function clientId(storage: SessionStorage): string {
  const stored = storage.getItem(clientIdKey);
  if (stored) {
    return stored;
  }
  const made = randomUuid();
  storage.setItem(clientIdKey, made);
  return made;
}
