import { describe, expect, test } from "vitest";
import type { WorkItem } from "../api/types";
import { orderItems } from "./itemOrder";

const item = (key: string, extra: Partial<WorkItem> = {}): WorkItem =>
  ({
    key,
    repo: "acme/api",
    number: Number(key.split("-")[1]),
    title: key,
    status: "open",
    ready: true,
    blocked: false,
    created_at: `2026-11-0${key.split("-")[1]}T10:00:00.000000Z`,
    updated_at: `2026-11-0${key.split("-")[1]}T10:00:00.000000Z`,
    ...extra,
  }) as WorkItem;

const keys = (items: WorkItem[]) => items.map((i) => i.key);

describe("orderItems", () => {
  const items = [
    item("PIN-3", { title: "Bravo", status: "done", updated_at: "2026-11-09T10:00:00.000000Z" }),
    item("PIN-1", { title: "Charlie", status: "in_progress", ready: false }),
    item("PIN-4", { title: "alpha", ready: false, blocked: true }),
    item("PIN-2", { title: "Delta" }),
    item("PIN-5", { title: "Echo", status: "cancelled", ready: false }),
  ];

  test("oldest first, by repository and number, is the default", () => {
    expect(keys(orderItems(items, "oldest"))).toEqual(["PIN-1", "PIN-2", "PIN-3", "PIN-4", "PIN-5"]);
  });

  test("newest first", () => {
    expect(keys(orderItems(items, "newest"))).toEqual(["PIN-5", "PIN-4", "PIN-3", "PIN-2", "PIN-1"]);
  });

  test("recently updated first, then the newest", () => {
    expect(keys(orderItems(items, "updated"))).toEqual(["PIN-3", "PIN-5", "PIN-4", "PIN-2", "PIN-1"]);
  });

  test("by status: in progress, ready, blocked, then done and cancelled, oldest first within each", () => {
    expect(keys(orderItems(items, "status"))).toEqual(["PIN-1", "PIN-2", "PIN-4", "PIN-3", "PIN-5"]);
  });

  test("by title, whatever its case", () => {
    expect(keys(orderItems(items, "title"))).toEqual(["PIN-4", "PIN-3", "PIN-1", "PIN-2", "PIN-5"]);
  });

  test("items of other repositories go by repository first when oldest", () => {
    const other = item("WEB-1", { repo: "acme/web" });
    expect(keys(orderItems([other, ...items.slice(0, 2)], "oldest"))).toEqual(["PIN-1", "PIN-3", "WEB-1"]);
  });
});
