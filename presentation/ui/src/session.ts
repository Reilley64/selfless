import { provideSession, signIn, signOut } from "@selfless/api-client";

export interface SessionStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

const tokenKey = "selfless.session.token";

export function startWebSession(storage: SessionStorage) {
  const listeners = new Set<() => void>();
  const changed = () => listeners.forEach((listener) => listener());
  const session = {
    token: () => storage.getItem(tokenKey) ?? undefined,
    ended(token: string) {
      if (token === session.token()) {
        storage.removeItem(tokenKey);
        changed();
      }
    },
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    async signIn(name: string, password: string, deviceName: string) {
      const response = await signIn({ name, password, clientKind: "web", deviceName });
      if (response.status === 201) {
        storage.setItem(tokenKey, response.data.token);
        changed();
      }
      return response;
    },
    async signOut() {
      const token = session.token();
      try {
        await signOut();
      } finally {
        if (token) {
          session.ended(token);
        }
      }
    },
  };
  provideSession(session);
  return session;
}
