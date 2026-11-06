// The typed client for /api/v1. Inside the app the shell asks Tauri for the
// server's URL; in a browser (development, tests) it uses VITE_PINRAIL_URL or
// the default port.

import type {
  CatalogEntry,
  Info,
  Inspection,
  ItemComment,
  ItemDetail,
  Notice,
  Plugin,
  Project,
  ProjectsListing,
  Review,
  ReviewEvent,
  ReviewListing,
  ReviewView,
  ServerSettings,
  Session,
  SessionComment,
  SessionItemEvent,
  SessionMessage,
  SessionStatus,
  Stage,
  ThemeEntry,
  Violation,
  WorkItem,
} from "./types";

export class ApiError extends Error {
  status: number;
  kind: string;
  violations: Violation[];
  constructor(status: number, body: unknown) {
    const said = (body ?? {}) as { message?: string; error?: string; violations?: unknown };
    super(said.message ?? `request failed (${status})`);
    this.status = status;
    this.kind = said.error ?? "error";
    this.violations = Array.isArray(said.violations) ? said.violations : [];
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
  if (!response.ok) throw new ApiError(response.status, await errorBody(response));
  const text = await response.text();
  return (text ? JSON.parse(text) : null) as T;
}

/** What a failed response says, when it is the server's JSON; a proxy's
 *  page or a cut-off body says nothing, and the status speaks for it. */
async function errorBody(response: Response): Promise<unknown> {
  try {
    return JSON.parse(await response.text());
  } catch {
    return null;
  }
}

/** An id or a name as one path segment. */
const seg = encodeURIComponent;

export type InstallRequest = { source: string; link?: boolean } | { id: string };

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
  /** a plugin's sample, sent as a new review: what `pinrail submit <plugin> --sample` does */
  sendSample: (plugin: string, body: { title?: string; sample?: string } = {}) =>
    request<Review>("POST", `/api/v1/plugins/${seg(plugin)}/sample`, body),
  /** lets a copied plugin's skill reach the connected agents, or stops it */
  allowSkill: (plugin: string, allowed: boolean) =>
    request<Plugin>("PATCH", `/api/v1/plugins/${seg(plugin)}/skill`, { allowed }),
  getReview: (id: string) => request<Review>("GET", `/api/v1/reviews/${seg(id)}`),
  /** the sessions, those waiting for the person first */
  sessions: (params: Record<string, string | undefined> = {}) =>
    request<{ sessions: Session[]; total: number; facets: { repos: string[]; unassigned: boolean } }>(
      "GET",
      `/api/v1/sessions${query(params)}`,
    ),
  /** the work items the filters keep: by default those open or in progress */
  items: (
    params: { status?: string; repo?: string; project?: string; q?: string; agent?: string; session?: string } = {},
  ) => request<{ items: WorkItem[] }>("GET", `/api/v1/items${query(params)}`),
  /** adds a work item, as the person */
  addItem: (item: { repo: string; title: string; body?: string; parent?: string }) =>
    request<WorkItem>("POST", "/api/v1/items", item),
  /** puts every round of a review under a work item, by its key, or under none */
  linkReview: (id: string, item: string | null) => request<void>("PUT", `/api/v1/reviews/${seg(id)}/item`, { item }),
  /** sets a repository's work item key prefix */
  setPrefix: (repo: string, prefix: string) => request<void>("PATCH", `/api/v1/repos/${seg(repo)}`, { prefix }),
  /** a work item with everything about it */
  item: (key: string) => request<ItemDetail>("GET", `/api/v1/items/${seg(key)}`),
  /** deletes a work item and its sub-items, as the person */
  deleteItem: (key: string) => request<{ deleted: number }>("DELETE", `/api/v1/items/${seg(key)}`),
  /** comments on a work item, as the person */
  commentItem: (key: string, body: string) =>
    request<ItemComment>("POST", `/api/v1/items/${seg(key)}/comments`, { body }),
  /** changes the text of the person's own comment */
  editComment: (key: string, id: string, body: string) =>
    request<ItemComment>("PATCH", `/api/v1/items/${seg(key)}/comments/${seg(id)}`, { body }),
  deleteComment: (key: string, id: string) => request<void>("DELETE", `/api/v1/items/${seg(key)}/comments/${seg(id)}`),
  /** changes a work item, whole or not at all, as the person */
  editItem: (key: string, change: Record<string, unknown>) =>
    request<{ item: ItemDetail; now_ready: { key: string; title: string }[] }>(
      "PATCH",
      `/api/v1/items/${seg(key)}`,
      change,
    ),
  /** the projects by name, and the repositories in none */
  projects: () => request<ProjectsListing>("GET", "/api/v1/projects"),
  /** creates a project, moving into it any of the repositories that are in another */
  createProject: (
    name: string,
    details: { description?: string; color?: string; emoji?: string } = {},
    repos: string[] = [],
  ) => request<Project>("POST", "/api/v1/projects", { name, ...details, repos }),
  /** changes what is given of a project; an empty description, colour or emoji removes it */
  updateProject: (id: string, change: { name?: string; description?: string; color?: string; emoji?: string }) =>
    request<Project>("PATCH", `/api/v1/projects/${seg(id)}`, change),
  /** sets the PNG a project's avatar shows, or removes it with null */
  setProjectImage: async (id: string, png: Blob | null) => {
    const response = await fetch(`${await serverUrl()}/api/v1/projects/${seg(id)}/image`, {
      method: png ? "PUT" : "DELETE",
      headers: png ? { "content-type": "image/png" } : {},
      body: png ?? undefined,
    });
    if (!response.ok) throw new ApiError(response.status, await errorBody(response));
    return (await response.json()) as Project;
  },
  /** deletes a project; its repositories are in none after it */
  deleteProject: (id: string) => request<void>("DELETE", `/api/v1/projects/${seg(id)}`),
  deleteRepo: (repo: string) =>
    request<{ repo: string; reviews: number; sessions: number; discarded: number }>(
      "DELETE",
      `/api/v1/repos/${seg(repo)}`,
    ),
  /** puts a repository in a project, out of the one it was in, or in none with null */
  moveRepo: (repo: string, project: string | null) =>
    request<void>("PUT", `/api/v1/repos/${seg(repo)}/project`, { project_id: project }),
  /** archives a session, or brings it back */
  archiveSession: (agent: string, id: string, archived: boolean) =>
    request<void>(archived ? "PUT" : "DELETE", `/api/v1/sessions/${seg(agent)}/${seg(id)}/archive`),
  /** the stages agents started, newest first, those whose names hold every word of q */
  stages: (params: { q?: string; limit?: string }) =>
    request<{ stages: Stage[] }>("GET", `/api/v1/stages${query(params)}`),
  /** a session and its questions and reviews, oldest first, its statuses, the comments its agent wrote on
   *  work items, and the person's messages */
  session: (agent: string, id: string) =>
    request<{
      session: Session;
      items: Review[];
      statuses: SessionStatus[];
      messages: SessionMessage[];
      comments?: SessionComment[];
      item_events?: SessionItemEvent[];
    }>("GET", `/api/v1/sessions/${seg(agent)}/${seg(id)}`),
  /** sends the agent a message, which it receives with its next status */
  sendMessage: (agent: string, id: string, text: string) =>
    request<SessionMessage>("POST", `/api/v1/sessions/${seg(agent)}/${seg(id)}/messages`, { text }),
  /** deletes a message the agent has not received yet */
  deleteMessage: (agent: string, id: string, message: number) =>
    request<void>("DELETE", `/api/v1/sessions/${seg(agent)}/${seg(id)}/messages/${message}`),
  rounds: (id: string) => request<Review[]>("GET", `/api/v1/reviews/${seg(id)}/rounds`),
  /** the decision on the version the view showed: a review that moved to another since is a conflict */
  decide: (id: string, data: unknown, agentNote: string, bundle?: string) =>
    request<Review>("POST", `/api/v1/reviews/${seg(id)}/decision`, { data, agent_note: agentNote, bundle }),
  withdraw: (id: string, reason?: string) =>
    request<Review>("POST", `/api/v1/reviews/${seg(id)}/withdraw`, reason ? { reason } : {}),
  /** the person's "no, and stop": nothing decided, the agent told */
  discard: (id: string, reason?: string) =>
    request<Review>("POST", `/api/v1/reviews/${seg(id)}/discard`, reason ? { reason } : {}),
  markViewed: (id: string) => request<void>("POST", `/api/v1/reviews/${seg(id)}/viewed`),
  /** stars a round of a review, or removes its star */
  deleteReview: (id: string) => request<{ ids: string[] }>("DELETE", `/api/v1/reviews/${seg(id)}`),
  star: (id: string, starred: boolean) => request<void>(starred ? "PUT" : "DELETE", `/api/v1/reviews/${seg(id)}/star`),
  events: (id: string) => request<ReviewEvent[]>("GET", `/api/v1/reviews/${seg(id)}/events`),
  plugins: () => request<{ plugins: Plugin[] }>("GET", "/api/v1/plugins"),
  /** what installing a folder or a zip would do, installing nothing */
  inspectPlugin: (body: InstallRequest) => request<Inspection>("POST", "/api/v1/plugins/inspect", body),
  /** installs a folder or a zip, or an official plugin by its id; the plugin's row */
  installPlugin: (body: InstallRequest) => request<Plugin>("POST", "/api/v1/plugins/install", body),
  /** the official plugins the app carries, each with the version installed */
  catalog: () => request<{ format: number; plugins: CatalogEntry[] }>("GET", "/api/v1/plugins/catalog"),
  /** opens a review for the app's frame, moving a pending one to the installed version when it takes it */
  reviewView: async (id: string) => {
    const view = await request<ReviewView>("POST", `/api/v1/reviews/${seg(id)}/view`);
    return { ...view, url: `${await serverUrl()}${view.url}` };
  },
  /** drops the installation; the bundles reviews render with stay with them */
  removePlugin: (name: string) =>
    request<{ removed: string; link: boolean; version: string }>("DELETE", `/api/v1/plugins/${seg(name)}`),
  themes: () => request<{ themes: ThemeEntry[] }>("GET", "/api/v1/themes"),
  createTheme: (id: string, from: string) => request<{ path: string }>("POST", "/api/v1/themes", { id, from }),
  settings: () => request<ServerSettings>("GET", "/api/v1/settings"),
  patchSettings: (patch: Record<string, unknown>) => request<ServerSettings>("PATCH", "/api/v1/settings", patch),
  /** the review rendered as markdown by the core, for the clipboard */
  reviewMarkdown: async (id: string) => {
    const response = await fetch(`${await serverUrl()}/api/v1/reviews/${seg(id)}?format=markdown`);
    if (!response.ok) throw new ApiError(response.status, await errorBody(response));
    return response.text();
  },
  /** a file a review carries, its bytes whole: for the view, which asks by name */
  attachmentBytes: async (id: string, name: string) => {
    const response = await fetch(`${await serverUrl()}/api/v1/reviews/${seg(id)}/attachments/${seg(name)}`);
    if (!response.ok) throw new ApiError(response.status, await errorBody(response));
    return response.arrayBuffer();
  },
};

/** Where feedback goes from a browser; the app sends it through its shell. */
const FEEDBACK_URL = import.meta.env.VITE_PINRAIL_FEEDBACK_URL ?? "https://api.pinrail.dev/v1/feedback";

/**
 * Sends the feedback form to the Pinrail team. Inside the app the shell sends
 * it, adding the app's version and system; in a browser (development,
 * tests) it goes to the service directly. Rejects with a sentence to show.
 */
export async function sendFeedback(form: FormData): Promise<void> {
  if (inTauri()) {
    // the form as it would go over HTTP: its bytes, and the boundary in its type
    const packed = new Response(form);
    const type = packed.headers.get("content-type") ?? "multipart/form-data";
    const bytes = new Uint8Array(await packed.arrayBuffer());
    const { invoke } = await import("@tauri-apps/api/core");
    try {
      await invoke("send_feedback", bytes, { headers: { "x-content-type": type } });
    } catch (e) {
      throw new Error(typeof e === "string" ? e : "The feedback was not sent.", { cause: e });
    }
    return;
  }
  let response: Response;
  try {
    response = await fetch(FEEDBACK_URL, {
      method: "POST",
      body: form,
      headers: { "x-pinrail-client": "pinrail/dev" },
    });
  } catch (e) {
    throw new Error("Pinrail could not reach the feedback service. Check your connection and try again.", {
      cause: e,
    });
  }
  if (!response.ok) {
    const said = ((await response.json().catch(() => null)) as { message?: string } | null)?.message;
    throw new Error(said ? `The feedback was not sent: ${said}.` : "The feedback was not sent.");
  }
}

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
    for (const kind of [
      "created",
      "decided",
      "withdrawn",
      "discarded",
      "expired",
      "viewed",
      "plugin_changed",
      "plugins_reloaded",
      "settings_changed",
      "themes_changed",
      "session_status",
      "session_message_sent",
      "session_message_deleted",
      "session_messages_delivered",
      "session_archived",
      "session_unarchived",
      "starred",
      "unstarred",
      "projects_changed",
      "repository_deleted",
      "review_deleted",
      "item_changed",
      "item_deleted",
      "history_swept",
    ]) {
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
