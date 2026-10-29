import { SpanKind, SpanStatusCode, context, propagation, trace } from "@opentelemetry/api";
import {
  ATTR_ERROR_TYPE,
  ATTR_HTTP_REQUEST_METHOD,
  ATTR_HTTP_RESPONSE_STATUS_CODE,
  ATTR_SERVER_ADDRESS,
  ATTR_SERVER_PORT,
  ATTR_URL_FULL,
} from "@opentelemetry/semantic-conventions";

export interface Session {
  token(): string | undefined;
  ended(token: string): void;
}

let session: Session = { token: () => undefined, ended: () => {} };

export function provideSession(provided: Session) {
  session = provided;
}

export function authorizationHeaders(token = session.token()): Record<string, string> {
  return token ? { Authorization: `Bearer ${token}` } : {};
}

export async function tracedFetch<T>(url: string, options: RequestInit): Promise<T> {
  const method = options.method ?? "GET";
  const href = `${process.env.EXPO_PUBLIC_SERVER_URL ?? ""}${url}`;
  const target = new URL(href, globalThis.location?.href);
  const span = trace.getTracer("@selfless/api-client").startSpan(method, {
    kind: SpanKind.CLIENT,
    attributes: {
      [ATTR_HTTP_REQUEST_METHOD]: method,
      [ATTR_URL_FULL]: target.href,
      [ATTR_SERVER_ADDRESS]: target.hostname,
      [ATTR_SERVER_PORT]: Number(target.port || (target.protocol === "https:" ? 443 : 80)),
    },
  });
  const headers = new Headers(options.headers);
  const token = session.token();
  for (const [name, value] of Object.entries(authorizationHeaders(token))) {
    headers.set(name, value);
  }
  propagation.inject(trace.setSpan(context.active(), span), headers, {
    set: (carrier, key, value) => carrier.set(key, value),
  });
  try {
    const response = await fetch(href, { ...options, headers });
    span.setAttribute(ATTR_HTTP_RESPONSE_STATUS_CODE, response.status);
    if (response.status === 401 && token) {
      session.ended(token);
    }
    if (response.status >= 400) {
      span.setAttribute(ATTR_ERROR_TYPE, String(response.status));
      span.setStatus({ code: SpanStatusCode.ERROR });
    }
    const body = [204, 205, 304].includes(response.status) ? null : await response.text();
    return { data: body ? JSON.parse(body) : {}, status: response.status, headers: response.headers } as T;
  } catch (error) {
    span.setAttribute(ATTR_ERROR_TYPE, error instanceof Error ? error.name : "_OTHER");
    span.setStatus({ code: SpanStatusCode.ERROR });
    throw error;
  } finally {
    span.end();
  }
}
