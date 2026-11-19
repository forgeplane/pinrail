// Work items for component tests, with what the core fills in.
import type { ItemDetail, WorkItem } from "../api/types";

let next = 1;

/** A work item of `repo`, open and ready unless `fields` say otherwise. */
export function workItem(fields: Partial<WorkItem> & { title: string }, repo = "acme/app"): WorkItem {
  const number = fields.number ?? next++;
  return {
    id: `wi_${number}`,
    key: `ACME-${number}`,
    number,
    repo,
    status: "open",
    parent: null,
    link: null,
    claim: null,
    created_by: "person",
    created_session: null,
    created_at: "2026-11-19T09:00:00Z",
    updated_at: "2026-11-19T09:00:00Z",
    closed_at: null,
    sub_items: { total: 0, closed: 0, in_progress: 0 },
    blocked_by: [],
    blocked: false,
    ready: true,
    project: null,
    waiting: 0,
    queue: null,
    ...fields,
  };
}

/** An item as its panel loads it: with its description, relations and
 *  timeline, empty unless `fields` give them. */
export function itemDetail(fields: Partial<ItemDetail> & { title: string }, repo = "acme/app"): ItemDetail {
  return {
    ...workItem(fields, repo),
    body: "",
    ancestors: [],
    depends_on: [],
    blocks: [],
    sub_items_list: [],
    files: [],
    reviews: [],
    sessions: [],
    timeline: [],
    ...fields,
  };
}

