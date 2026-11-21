// The typed client for /api/v1. Inside the app the shell asks Tauri for the
// server's URL and the person token. In a browser (development, tests) it uses
// the server set as window.__PINRAIL_SERVER__ before the page loads, which the
// end-to-end tests set for the core of each worker, else VITE_PINRAIL_URL or
// the default port, and VITE_PINRAIL_TOKEN.

import type { Attached } from "../lib/attach";
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
  TelemetryStatus,
  Session,
  SessionComment,
  SessionItemEvent,
  SessionMessage,
  SessionStatus,
  SessionTurn,
  Stage,
  Element,
  ThemeEntry,
  Violation,
  WorkItem,
  FoundItem,
  InboxCounts,
  ItemStatus,
  NotificationList,
  NotificationTab,
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

// every status an item can have, as a record so that a status added to
// ItemStatus has to be added here too
const itemStatuses: Record<ItemStatus, true> = {
  open: true,
  in_progress: true,
  on_hold: true,
  done: true,
  cancelled: true,
};
/** The `status` that lists items whatever their status. */
export const EVERY_ITEM_STATUS = Object.keys(itemStatuses).join(",");

/** What went wrong, for the person: the core's own words, the first
 *  violation's when it refused a field, or `fallback` when the core could
 *  not be reached or the error is not the core's. */
export function errorMessage(error: unknown, fallback = "Pinrail could not be reached."): string {
  if (!(error instanceof ApiError)) return fallback;
  return error.violations[0]?.message ?? error.message;
}

type Server = { url: string; token: string };
let server: Promise<Server> | undefined;

export const inTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function serverInfo(): Promise<Server> {
  server ??= (async () => {
    if (inTauri()) {
      const { invoke } = await import("@tauri-apps/api/core");
      return invoke<Server>("server_url");
    }
    const given =
      typeof window === "undefined" ? undefined : (window as { __PINRAIL_SERVER__?: Server }).__PINRAIL_SERVER__;
    return (
      given ?? {
        url: import.meta.env.VITE_PINRAIL_URL ?? "http://127.0.0.1:4747",
        token: import.meta.env.VITE_PINRAIL_TOKEN ?? "",
      }
    );
  })();
  return server;
}

export async function serverUrl(): Promise<string> {
  return (await serverInfo()).url;
}

/** The headers of a write: the core takes a write from the window only
 *  with the person token, which proves that the person made it. Reads go
 *  without it. */
async function writeHeaders(contentType?: string): Promise<Record<string, string>> {
  const headers: Record<string, string> = { authorization: `Bearer ${(await serverInfo()).token}` };
  if (contentType) headers["content-type"] = contentType;
  return headers;
}

/** A call to the core's API, as JSON. A write carries the person token,
 *  and so does a read with `asPerson`, of what only the person reads. */
export async function request<T>(
  method: string,
  path: string,
  body?: unknown,
  { asPerson = false }: { asPerson?: boolean } = {},
): Promise<T> {
  const response = await fetch(`${await serverUrl()}${path}`, {
    method,
    // the core refuses a write that does not say it is JSON, even an empty one
    headers: method === "GET" ? (asPerson ? await writeHeaders() : {}) : await writeHeaders("application/json"),
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
  /** the work items the filters keep, without their descriptions: by
   *  default those open or in progress */
  items: (
    params: {
      status?: string;
      repo?: string;
      project?: string;
      q?: string;
      agent?: string;
      session?: string;
      queued?: "true";
      /** `title`: `q` is looked for in the titles only */
      in?: "title";
    } = {},
  ) => request<{ items: WorkItem[]; next: string[] }>("GET", `/api/v1/items${query({ ...params, fields: "summary" })}`),
  /** the items whose title, description or comments match `q`, best first */
  searchItems: (params: { q: string; status?: string; repo?: string; project?: string; limit?: number }) =>
    request<{ items: FoundItem[] }>(
      "GET",
      `/api/v1/items/search${query({ ...params, limit: params.limit?.toString() })}`,
    ),
  /** puts an item in the person's queue, or moves it: at the end, or
   *  before or after a queued item; answers the queue in order */
  queueItem: (key: string, place: { before?: string; after?: string } = {}) =>
    request<{ items: WorkItem[] }>("PUT", `/api/v1/items/${seg(key)}/queue`, place),
  /** takes an item out of the person's queue; answers the queue in order */
  unqueueItem: (key: string) => request<{ items: WorkItem[] }>("DELETE", `/api/v1/items/${seg(key)}/queue`),
  /** adds a work item, as the person */
  addItem: (item: {
    repo: string;
    title: string;
    body?: string;
    /** the files the body shows, by the names it links them under */
    attachments?: Record<string, Attached>;
    parent?: string;
  }) => request<WorkItem>("POST", "/api/v1/items", item),
  /** puts every round of a review under exactly these work items, by their keys */
  setReviewItems: (id: string, items: string[]) => request<void>("PUT", `/api/v1/reviews/${seg(id)}/items`, { items }),
  /** sets a repository's work item key prefix */
  setPrefix: (repo: string, prefix: string) => request<void>("PATCH", `/api/v1/repos/${seg(repo)}`, { prefix }),
  /** a work item with everything about it */
  item: (key: string) => request<ItemDetail>("GET", `/api/v1/items/${seg(key)}`),
  /** deletes a work item and its sub-items, as the person */
  deleteItem: (key: string) => request<{ deleted: number }>("DELETE", `/api/v1/items/${seg(key)}`),
  /** comments on a work item, as the person */
  commentItem: (key: string, body: string, attachments?: Record<string, unknown>) =>
    request<ItemComment>("POST", `/api/v1/items/${seg(key)}/comments`, {
      body,
      ...(attachments && Object.keys(attachments).length ? { attachments } : {}),
    }),
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
  /** stores a file's bytes under their SHA-256, for a description or a
   *  comment to carry; the core keeps one copy of the same bytes */
  uploadFile: async (file: File): Promise<{ sha256: string; size: number; media_type: string }> => {
    const bytes = await file.arrayBuffer();
    const digest = await crypto.subtle.digest("SHA-256", bytes);
    const sha256 = [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
    const response = await fetch(`${await serverUrl()}/api/v1/attachments/${sha256}`, {
      method: "PUT",
      headers: await writeHeaders("application/octet-stream"),
      body: bytes,
    });
    if (!response.ok) throw new ApiError(response.status, await errorBody(response));
    return { sha256, size: bytes.byteLength, media_type: file.type || "application/octet-stream" };
  },
  /** sets the PNG a project's avatar shows, or removes it with null */
  setProjectImage: async (id: string, png: Blob | null) => {
    const response = await fetch(`${await serverUrl()}/api/v1/projects/${seg(id)}/image`, {
      method: png ? "PUT" : "DELETE",
      headers: await writeHeaders(png ? "image/png" : undefined),
      body: png ?? undefined,
    });
    if (!response.ok) throw new ApiError(response.status, await errorBody(response));
    return (await response.json()) as Project;
  },
  /** deletes a project; its repositories are in none after it */
  deleteProject: (id: string) => request<void>("DELETE", `/api/v1/projects/${seg(id)}`),
  deleteRepo: (repo: string) =>
    request<{ repo: string; reviews: number; sessions: number; items: number; discarded: number }>(
      "DELETE",
      `/api/v1/repos/${seg(repo)}`,
    ),
  /** puts a repository in a project, out of the one it was in, or in none with null */
  moveRepo: (repo: string, project: string | null) =>
    request<void>("PUT", `/api/v1/repos/${seg(repo)}/project`, { project_id: project }),
  /** archives a session, or brings it back */
  archiveSession: (agent: string, id: string, archived: boolean) =>
    request<void>(archived ? "PUT" : "DELETE", `/api/v1/sessions/${seg(agent)}/${seg(id)}/archive`),
  /** renames a session, or with null gives it back the name Pinrail gave it */
  renameSession: (agent: string, id: string, name: string | null) =>
    request<void>("PATCH", `/api/v1/sessions/${seg(agent)}/${seg(id)}`, { name }),
  /** deletes a session: stops its run, withdraws what waits, gives up its items */
  deleteSession: (agent: string, id: string) => request<void>("DELETE", `/api/v1/sessions/${seg(agent)}/${seg(id)}`),
  /** Marks a session seen, which clears Finished, or unread, which brings it back. */
  setSeen: (agent: string, id: string, seen: boolean) =>
    request<void>(seen ? "PUT" : "DELETE", `/api/v1/sessions/${seg(agent)}/${seg(id)}/seen`),
  /** the chemical elements sessions are named after, by atomic number */
  elements: () => request<{ elements: Element[] }>("GET", "/api/v1/elements"),
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
      turns?: SessionTurn[];
    }>("GET", `/api/v1/sessions/${seg(agent)}/${seg(id)}`),
  /** sends the agent a message: typed into its terminal when Pinrail runs
   *  it, or with its next status; a `local_id` makes a resend the same
   *  message */
  sendMessage: (agent: string, id: string, text: string, localId?: string) =>
    request<SessionMessage>("POST", `/api/v1/sessions/${seg(agent)}/${seg(id)}/messages`, {
      text,
      local_id: localId,
    }),
  /** types an unconfirmed message into the agent's terminal again */
  typeMessage: (agent: string, id: string, message: number) =>
    request<void>("POST", `/api/v1/sessions/${seg(agent)}/${seg(id)}/messages/${message}/type`, {}),
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
  /** a page of the person's notifications, newest first */
  notifications: (params: { tab?: NotificationTab; limit?: number; offset?: number } = {}) =>
    request<NotificationList>(
      "GET",
      `/api/v1/notifications${query({
        tab: params.tab,
        limit: params.limit?.toString(),
        offset: params.offset?.toString(),
      })}`,
    ),
  notificationCounts: () => request<InboxCounts>("GET", "/api/v1/notifications/counts"),
  /** marks a notification read, or unread */
  setNotificationRead: (id: number, read: boolean) =>
    request<void>(read ? "PUT" : "DELETE", `/api/v1/notifications/${id}/read`),
  /** marks a notification done, or takes it out of done */
  setNotificationDone: (id: number, done: boolean) =>
    request<void>(done ? "PUT" : "DELETE", `/api/v1/notifications/${id}/done`),
  /** marks read what a list showed, up to the newest event it reflected */
  readNotifications: (tab: NotificationTab, upTo: number) =>
    request<{ read: number }>("POST", "/api/v1/notifications/read", { tab, up_to: upTo }),
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
  telemetry: () => request<TelemetryStatus>("GET", "/api/v1/telemetry"),
  /** keeps an uncaught error as a crash report, while crash reports are on */
  reportCrash: (message: string, stack: string) =>
    request<void>("POST", "/api/v1/telemetry/crashes", { message, stack }),
  /** counts a feature of the window in the usage data, while it is on */
  countFeature: (feature: string) => request<void>("POST", "/api/v1/telemetry/features", { feature }),
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
/** How soon the app tries the events again after an error: at first soon,
 *  since at startup the window may load before the server listens, then
 *  less often. The browser's own retry waits three seconds. */
const RETRY_FIRST_MS = 250;
const RETRY_MAX_MS = 3000;

export function subscribe(handlers: {
  onNotice: (notice: Notice) => void;
  onOpen?: () => void;
  onError?: () => void;
}): () => void {
  let source: EventSource | undefined;
  let closed = false;
  let timer: number | undefined;
  let delay = RETRY_FIRST_MS;
  // the last event seen, so that a new connection catches up from it
  let last: string | null = null;
  const notice = (event: MessageEvent) => {
    if (event.lastEventId) last = event.lastEventId;
    try {
      handlers.onNotice(JSON.parse(event.data));
    } catch {
      // a malformed event is dropped; the next refresh catches up
    }
  };
  const connect = (url: string) => {
    if (closed) return;
    source = new EventSource(`${url}/api/v1/events${last ? `?after=${encodeURIComponent(last)}` : ""}`);
    source.onopen = () => {
      delay = RETRY_FIRST_MS;
      handlers.onOpen?.();
    };
    // the app retries itself, sooner than the browser would
    source.onerror = () => {
      handlers.onError?.();
      source?.close();
      if (closed) return;
      timer = window.setTimeout(() => connect(url), delay);
      delay = Math.min(delay * 2, RETRY_MAX_MS);
    };
    source.onmessage = notice;
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
      "session_message_state",
      "session_presence",
      "session_turn",
      "session_seen",
      "session_archived",
      "session_unarchived",
      "session_renamed",
      "session_deleted",
      "run_started",
      "run_bound",
      "run_ended",
      "starred",
      "unstarred",
      "projects_changed",
      "repository_deleted",
      "review_deleted",
      "item_changed",
      "prefix_changed",
      "item_deleted",
      "history_swept",
      "notification_changed",
      // the stored notices no longer reach back to the last one seen
      "resync",
    ]) {
      source.addEventListener(kind, (event) => notice(event as MessageEvent));
    }
  };
  serverUrl().then(connect);
  return () => {
    closed = true;
    window.clearTimeout(timer);
    source?.close();
  };
}
