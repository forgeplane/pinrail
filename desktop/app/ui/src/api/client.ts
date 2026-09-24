// The typed client for /api/v1. Inside the app the shell asks Tauri for the
// server's URL; in a browser (development, tests) it uses VITE_PINRAIL_URL or
// the default port.

import type { Info, InstallJob, Inspection, Notice, Plugin, PluginUpdates, Review, ReviewEvent, ReviewListing, ServerSettings, Violation } from "./types";

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

export const inTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function serverUrl(): Promise<string> {
  baseUrl ??= (async () => {
    if (inTauri()) {
      const { invoke } = await import("@tauri-apps/api/core");
      return invoke<string>("server_url");
    }
    return import.meta.env.VITE_PINRAIL_URL ?? "http://127.0.0.1:4747";
  })();
  return baseUrl;
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const response = await fetch(`${await serverUrl()}${path}`, {
    method,
    // the core refuses a write that does not say it is JSON, even an empty one
    headers: method === "GET" ? {} : { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (response.status === 204) return undefined as T;
  const text = await response.text();
  const parsed = text ? JSON.parse(text) : null;
  if (!response.ok) throw new ApiError(response.status, parsed);
  return parsed as T;
}

export type InstallRequest = { source: string; link?: boolean; force?: boolean; ref?: string; path?: string };

const query = (params: Record<string, string | undefined>) => {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) if (v) q.set(k, v);
  const s = q.toString();
  return s ? `?${s}` : "";
};

export const api = {
  info: () => request<Info>("GET", "/api/v1/info"),
  listReviews: (params: Record<string, string | undefined> = {}) =>
    request<ReviewListing>("GET", `/api/v1/reviews${query(params)}`),
  getReview: (id: string) => request<Review>("GET", `/api/v1/reviews/${id}`),
  rounds: (id: string) => request<Review[]>("GET", `/api/v1/reviews/${id}/rounds`),
  decide: (id: string, data: unknown, agentNote: string) =>
    request<Review>("POST", `/api/v1/reviews/${id}/decision`, { data, agent_note: agentNote }),
  withdraw: (id: string, reason?: string) =>
    request<Review>("POST", `/api/v1/reviews/${id}/withdraw`, reason ? { reason } : {}),
  /** the person's "no, and stop": nothing decided, the agent told */
  discard: (id: string, reason?: string) =>
    request<Review>("POST", `/api/v1/reviews/${id}/discard`, reason ? { reason } : {}),
  markViewed: (id: string) => request<void>("POST", `/api/v1/reviews/${id}/viewed`),
  events: (id: string) => request<ReviewEvent[]>("GET", `/api/v1/reviews/${id}/events`),
  plugins: () => request<{ plugins: Plugin[] }>("GET", "/api/v1/plugins"),
  reloadPlugins: () => request<{ ok: boolean; count: number }>("POST", "/api/v1/plugins/reload"),
  /** what installing a source would do; the source is fetched and dropped */
  inspectPlugin: (body: InstallRequest) => request<Inspection>("POST", "/api/v1/plugins/inspect", body),
  /** starts an install; the job says how it goes */
  installPlugin: (body: InstallRequest) => request<{ job: string }>("POST", "/api/v1/plugins/install", body),
  pluginJob: (id: string) => request<InstallJob>("GET", `/api/v1/plugins/jobs/${id}`),
  pluginUpdates: (name: string) => request<PluginUpdates>("GET", `/api/v1/plugins/${name}/updates`),
  /** the majors of a plugin that reviews can still render with; 404 for an unknown plugin */
  pluginVersions: (name: string) => request<{ name: string; current: number | null; versions: number[] }>("GET", `/api/v1/plugins/${name}/versions`),
  /** installs again from where it came: a job to follow, or up_to_date at once */
  updatePlugin: (name: string) => request<{ job?: string; state: string; version?: string }>("POST", `/api/v1/plugins/${name}/update`),
  /** drops the record and the store entries no review renders from */
  removePlugin: (name: string) => request<{ removed: string; linked: boolean; entries_kept: number[] }>("DELETE", `/api/v1/plugins/${name}`),
  settings: () => request<ServerSettings>("GET", "/api/v1/settings"),
  patchSettings: (patch: Record<string, unknown>) => request<ServerSettings>("PATCH", "/api/v1/settings", patch),
  /** The URL a plugin's bundle is loaded from; the iframe adds the theme. */
  /** the review rendered as markdown by the core, for the clipboard */
  reviewMarkdown: async (id: string) => {
    const response = await fetch(`${await serverUrl()}/api/v1/reviews/${id}?format=markdown`);
    if (!response.ok) throw new ApiError(response.status, null);
    return response.text();
  },
  /** a file a review carries, its bytes whole: for the view, which asks by name */
  attachmentBytes: async (id: string, name: string) => {
    const response = await fetch(`${await serverUrl()}/api/v1/reviews/${id}/attachments/${encodeURIComponent(name)}`);
    if (!response.ok) {
      const text = await response.text();
      let body = null;
      try {
        body = JSON.parse(text);
      } catch {
        // not JSON: the status says enough
      }
      throw new ApiError(response.status, body);
    }
    return response.arrayBuffer();
  },
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
    for (const kind of ["created", "decided", "withdrawn", "discarded", "expired", "viewed", "plugins_reloaded", "settings_changed"]) {
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
