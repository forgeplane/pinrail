// What the shell specs share: where the core listens, and reviews submitted,
// decided and cleared through its API, as an agent or the app would.

import { expect, type APIRequestContext } from "@playwright/test";

/** The port of the headless core the shell runs against. */
export const corePort = 4799;
export const core = `http://127.0.0.1:${corePort}`;

/** A list review with one proposal, for when the content does not matter. */
export const onePayload = {
  intro: "One proposal.",
  groups: [{ title: "lib/acme/tickets.ex", items: [{ id: 1, severity: "minor", title: "moduledoc typo" }] }],
};

type Review = {
  title: string;
  plugin?: string;
  origin?: Record<string, string>;
  payload?: unknown;
  revises?: string;
};

/** Submits a review as an agent would: a list with one proposal unless told otherwise. */
export async function createReview(
  request: APIRequestContext,
  { title, plugin = "list", origin = { repo: "acme/api" }, payload = onePayload, revises }: Review,
): Promise<{ id: string }> {
  const response = await request.post(`${core}/api/v1/reviews`, {
    data: { plugin, title, origin, requested_by: "spec", payload, revises },
  });
  expect(response.status(), await response.text()).toBe(201);
  return (await response.json()) as { id: string };
}

/** Records a decision as the app does when a person submits one. */
export async function decide(request: APIRequestContext, id: string, data: unknown) {
  const response = await request.post(`${core}/api/v1/reviews/${id}/decision`, { data: { data } });
  expect(response.status(), await response.text()).toBe(200);
}

/** Discards whatever is pending, so a test starts from an empty inbox. */
export async function clearInbox(request: APIRequestContext) {
  const pending = (await (await request.get(`${core}/api/v1/reviews?status=pending&limit=500`)).json()).reviews as { id: string }[];
  for (const r of pending) {
    const response = await request.post(`${core}/api/v1/reviews/${r.id}/discard`, { data: { reason: "spec cleanup" } });
    expect(response.status(), `discarding ${r.id}: ${await response.text()}`).toBe(200);
  }
}
