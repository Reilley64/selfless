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
import { flushTelemetry } from "./telemetry/startTelemetry";

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
  dimmed?: boolean;
}

function listeners() {
  const subscribed = new Set<() => void>();
  return {
    changed: () => subscribed.forEach((listener) => listener()),
    subscribe(listener: () => void) {
      subscribed.add(listener);
      return () => {
        subscribed.delete(listener);
      };
    },
  };
}

export function startWebSession(storage: SessionStorage) {
  const { changed, subscribe } = listeners();
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
      await flushTelemetry();
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
  const { changed, subscribe } = listeners();
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
  const endSession = async (token: string) => {
    await flushTelemetry();
    const response = await signOut({ headers: authorizationHeaders(token) }).catch(() => undefined);
    if (response?.status === 204) {
      session.ended(token);
    }
    return response?.status === 204 || response?.status === 401;
  };
  const session = {
    state: () => state,
    subscribe,
    serverUrl: () => state.serverUrl ?? "",
    token: () => state.users.find(({ id }) => id === state.currentUserId)?.token,
    telemetryToken: () =>
      (state.users.find(({ id }) => id === state.currentUserId) ?? state.users.find(({ dimmed }) => !dimmed))?.token,
    ended(token: string) {
      const owner = state.users.find((user) => user.token === token);
      if (owner) {
        const users = state.users.map((user) => (user === owner ? { ...user, dimmed: true } : user));
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
        if (previous && !(await endSession(previous.token))) {
          await endSession(signedIn.token);
          throw new Error("The old session could not be ended");
        }
        const users = previous
          ? state.users.map((user) => (user.id === signedIn.id ? signedIn : user))
          : [...state.users, signedIn];
        updateUsers(users, { currentUserId: signedIn.id });
      }
      return response;
    },
    async refresh() {
      await Promise.all(
        state.users
          .filter(({ dimmed }) => !dimmed)
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
      if (removed && !(await endSession(removed.token))) {
        return false;
      }
      const users = state.users.filter(({ id }) => id !== userId);
      updateUsers(users);
      if (users.length === 0) {
        session.forgetServer();
      }
      return true;
    },
  };
  provideSession(session);
  return session;
}
