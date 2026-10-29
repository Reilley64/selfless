import { getGetInfoUrl, provideSession, signIn, signOut, type User } from "@selfless/api-client";

export interface SessionStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

const tokenKey = "selfless.session.token";
const serverUrlKey = "selfless.server.url";
const usersKey = "selfless.session.users";

export interface PickerUser extends User {
  token: string;
}

function changes() {
  const listeners = new Set<() => void>();
  return {
    changed: () => listeners.forEach((listener) => listener()),
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}

export function startWebSession(storage: SessionStorage) {
  const { changed, subscribe } = changes();
  const session = {
    serverUrl: () => "",
    token: () => storage.getItem(tokenKey) ?? undefined,
    ended(token: string) {
      if (token === session.token()) {
        storage.removeItem(tokenKey);
        changed();
      }
    },
    subscribe,
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

export function startTvSession(storage: SessionStorage, clientKind: "appleTv" | "googleTv") {
  const { changed, subscribe } = changes();
  let state = {
    serverUrl: storage.getItem(serverUrlKey) ?? undefined,
    users: JSON.parse(storage.getItem(usersKey) ?? "[]") as PickerUser[],
    currentUserId: undefined as string | undefined,
  };
  const update = (next: Partial<typeof state>) => {
    state = { ...state, ...next };
    changed();
  };
  const session = {
    state: () => state,
    subscribe,
    serverUrl: () => state.serverUrl ?? "",
    token: () => state.users.find(({ id }) => id === state.currentUserId)?.token,
    ended: () => {},
    pick(userId: string) {
      update({ currentUserId: userId });
    },
    switchUser() {
      update({ currentUserId: undefined });
    },
    async connect(address: string) {
      const typed = address.trim().replace(/\/+$/, "");
      const serverUrl = /^https?:\/\//.test(typed) ? typed : `http://${typed}`;
      const response = await fetch(`${serverUrl}${getGetInfoUrl()}`).catch(() => undefined);
      if (!response?.ok) {
        return false;
      }
      storage.setItem(serverUrlKey, serverUrl);
      update({ serverUrl });
      return true;
    },
    async signIn(name: string, password: string, deviceName: string) {
      const response = await signIn({ name, password, clientKind, deviceName });
      if (response.status === 201) {
        const users = [...state.users, { token: response.data.token, ...response.data.user }];
        storage.setItem(usersKey, JSON.stringify(users));
        update({ users, currentUserId: response.data.user.id });
      }
      return response;
    },
  };
  provideSession(session);
  return session;
}
