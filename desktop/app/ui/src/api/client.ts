// The typed client for /api/v1. Inside the app the shell asks Tauri for the
// server's URL; in a browser (development, tests) it uses VITE_WICKET_URL or
// the default port.

import type { Info, Notice, Plugin, Review, Violation } from "./types";

export class ApiError extends Error {
  status: number;
  kind: string;
  violations: Violation[];
  constructor(status: number, body: any) {
    super(body?.message ?? `request failed (${status})`);
    this.status = status;
    this.kind = body?.error ?? "error";
    this.violations = Array.isArray(body?.violations) ? body.violations : [];
  }
}

let baseUrl: Promise<string> | undefined;

const inTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function serverUrl(): Promise<string> {
  baseUrl ??= (async () => {
    if (inTauri()) {
      const { invoke } = await import("@tauri-apps/api/core");
      return invoke<string>("server_url");
    }
    return import.meta.env.VITE_WICKET_URL ?? "http://127.0.0.1:4747";
  })();
  return baseUrl;
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const response = await fetch(`${await serverUrl()}${path}`, {
    method,
    headers: body === undefined ? {} : { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (response.status === 204) return undefined as T;
  const text = await response.text();
  const parsed = text ? JSON.parse(text) : null;
  if (!response.ok) throw new ApiError(response.status, parsed);
  return parsed as T;
}

const query = (params: Record<string, string | undefined>) => {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) if (v) q.set(k, v);
  const s = q.toString();
  return s ? `?${s}` : "";
};

export const api = {
  info: () => request<Info>("GET", "/api/v1/info"),
  listReviews: (params: Record<string, string | undefined> = {}) =>
    request<Review[]>("GET", `/api/v1/reviews${query(params)}`),
  getReview: (id: string) => request<Review>("GET", `/api/v1/reviews/${id}`),
  rounds: (id: string) => request<Review[]>("GET", `/api/v1/reviews/${id}/rounds`),
  decide: (id: string, data: unknown, agentNote: string) =>
    request<Review>("POST", `/api/v1/reviews/${id}/decision`, { data, agent_note: agentNote }),
  withdraw: (id: string, reason?: string) =>
    request<Review>("POST", `/api/v1/reviews/${id}/withdraw`, reason ? { reason } : {}),
  markViewed: (id: string) => request<void>("POST", `/api/v1/reviews/${id}/viewed`),
  plugins: () => request<{ dirs: string[]; plugins: Plugin[] }>("GET", "/api/v1/plugins"),
  reloadPlugins: () => request<{ ok: boolean; count: number }>("POST", "/api/v1/plugins/reload"),
  addPluginDir: (dir: string) =>
    request<{ ok: boolean; count: number; dirs: string[] }>("POST", "/api/v1/plugins/dirs", { dir }),
  /** The URL a plugin's bundle is loaded from; the iframe adds the theme. */
  bundleUrl: async (review: Review, entry: string) =>
    `${await serverUrl()}/plugins/${review.plugin}/${review.plugin_version}/${entry}`,
};

/** Subscribes to the server's event stream. Returns a function that closes it. */
export function subscribe(handlers: {
  onNotice: (notice: Notice) => void;
  onOpen?: () => void;
  onError?: () => void;
}): () => void {
  let source: EventSource | undefined;
  let closed = false;
  serverUrl().then((url) => {
    if (closed) return;
    source = new EventSource(`${url}/api/v1/events`);
    source.onopen = () => handlers.onOpen?.();
    source.onerror = () => handlers.onError?.();
    source.onmessage = (event) => {
      try {
        handlers.onNotice(JSON.parse(event.data));
      } catch {
        // a malformed event is dropped; the next refresh catches up
      }
    };
    for (const kind of ["created", "decided", "withdrawn", "expired", "viewed", "plugins_reloaded"]) {
      source.addEventListener(kind, (event) => {
        try {
          handlers.onNotice(JSON.parse((event as MessageEvent).data));
        } catch {
          // as above
        }
      });
    }
  });
  return () => {
    closed = true;
    source?.close();
  };
}
