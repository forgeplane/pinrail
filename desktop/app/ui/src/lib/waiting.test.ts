import { describe, expect, test } from "vitest";
import type { Review } from "../api/types";
import { filtering, matches, waitingFilter } from "./waiting";

const review = (over: Partial<Review> = {}) =>
  ({
    id: "r_1",
    kind: "review",
    plugin: "list",
    title: "Theme loader",
    requested_by: "claude-code",
    origin: { repo: "acme/api", workflow: "nightly", ref: "main" },
    project: { id: "p1", name: "Acme" },
    starred: false,
    ...over,
  }) as unknown as Review;
const params = (query: string) => new URLSearchParams(query);

describe("matches", () => {
  test("finds the text in what the row shows, whatever its case", () => {
    for (const q of ["theme", "LIST", "claude", "acme/api", "nightly", "MAIN"]) expect(matches(review(), q)).toBe(true);
    expect(matches(review({ kind: "question", plugin: "" }), "questions")).toBe(true);
    expect(matches(review(), "deploy")).toBe(false);
  });
  test("keeps everything for an empty search", () => {
    expect(matches(review(), "")).toBe(true);
  });
});

describe("waitingFilter", () => {
  test("keeps a review that passes every filter", () => {
    expect(waitingFilter(params("project=p1&plugin=list"), "theme")(review())).toBe(true);
  });
  test("leaves out a review outside the scope, of another plugin, or not starred", () => {
    expect(waitingFilter(params("project=p2"), "")(review())).toBe(false);
    expect(waitingFilter(params("repo=acme/web"), "")(review())).toBe(false);
    expect(waitingFilter(params("plugin=feedback"), "")(review())).toBe(false);
    expect(waitingFilter(params("starred=1"), "")(review())).toBe(false);
    expect(waitingFilter(params("starred=1"), "")(review({ starred: true }))).toBe(true);
  });
});

test("filtering is whether any filter is set", () => {
  expect(filtering(params(""), "")).toBe(false);
  for (const [query, q] of [
    ["", "theme"],
    ["repo=acme/api", ""],
    ["project=p1", ""],
    ["plugin=list", ""],
    ["starred=1", ""],
  ])
    expect(filtering(params(query), q)).toBe(true);
});
