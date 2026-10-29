import {
  authorizationHeaders,
  ClientKind,
  getGetInfoUrl,
  getMe,
  provideSession,
  signIn,
  signOut,
  type User,
} from "@selfless/api-client";

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
  signedOut?: boolean;
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
      const response = await signIn({ name, password, clientKind: ClientKind.web, deviceName });
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

export function startTvSession(storage: SessionStorage, clientKind: Exclude<ClientKind, typeof ClientKind.web>) {
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
  const updateUsers = (users: PickerUser[], next: Partial<typeof state> = {}) => {
    storage.setItem(usersKey, JSON.stringify(users));
    update({ users, ...next });
  };
  const session = {
    state: () => state,
    subscribe,
    serverUrl: () => state.serverUrl ?? "",
    token: () => state.users.find(({ id }) => id === state.currentUserId)?.token,
    ended(token: string) {
      const owner = state.users.find((user) => user.token === token);
      if (owner) {
        const users = state.users.map((user) => (user === owner ? { ...user, signedOut: true } : user));
        updateUsers(users, owner.id === state.currentUserId ? { currentUserId: undefined } : {});
      }
    },
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
    forgetServer() {
      storage.removeItem(serverUrlKey);
      update({ serverUrl: undefined });
    },
    async signIn(name: string, password: string, deviceName: string) {
      const response = await signIn({ name, password, clientKind, deviceName });
      if (response.status === 201) {
        const signedIn = { token: response.data.token, ...response.data.user };
        const previous = state.users.find(({ id }) => id === signedIn.id);
        const users = previous
          ? state.users.map((user) => (user === previous ? signedIn : user))
          : [...state.users, signedIn];
        updateUsers(users, { currentUserId: signedIn.id });
        if (previous) {
          await signOut({ headers: authorizationHeaders(previous.token) });
        }
      }
      return response;
    },
    async refresh() {
      await Promise.all(
        state.users
          .filter(({ signedOut }) => !signedOut)
          .map(async ({ id, token }) => {
            const response = await getMe({ headers: authorizationHeaders(token) });
            if (response.status === 200) {
              updateUsers(state.users.map((user) => (user.id === id ? { ...user, ...response.data } : user)));
            }
          }),
      );
    },
    async remove(userId: string) {
      const removed = state.users.find(({ id }) => id === userId);
      if (!removed) {
        return;
      }
      try {
        await signOut({ headers: authorizationHeaders(removed.token) });
      } finally {
        const users = state.users.filter(({ id }) => id !== userId);
        updateUsers(users);
        if (users.length === 0) {
          session.forgetServer();
        }
      }
    },
  };
  provideSession(session);
  return session;
}
