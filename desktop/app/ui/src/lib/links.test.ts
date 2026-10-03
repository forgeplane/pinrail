import { describe, expect, it } from "vitest";
import type { Plugin, PluginInstall } from "../api/types";
import { allowedWithoutAsking, allowing, linkRequest, sourceOf } from "./links";

const plugin = (install: Partial<PluginInstall>) =>
  ({
    name: "review",
    install: { source_kind: "folder", source: "/code/review", link: false, ...install },
  }) as Plugin;

const request = linkRequest("https://github.com/acme/api/pull/7")!;

describe("link permissions", () => {
  it("stay with a plugin from disk, whatever folder or zip it came from", () => {
    const granted = allowing(undefined, sourceOf(plugin({})), request.origin!);
    for (const upgraded of [
      plugin({ source_kind: "archive", source: "/Downloads/review-1.3.0.zip" }),
      plugin({ source: "/elsewhere/review" }),
      plugin({ link: true }),
    ]) {
      expect(allowedWithoutAsking(request, granted, sourceOf(upgraded))).toBe(true);
    }
  });

  it("are not carried between the app's own copy and a plugin from disk", () => {
    const fromDisk = allowing(undefined, sourceOf(plugin({})), request.origin!);
    const app = plugin({ source_kind: "app", source: "" });
    expect(allowedWithoutAsking(request, fromDisk, sourceOf(app))).toBe(false);
    const fromApp = allowing(undefined, sourceOf(app), request.origin!);
    expect(allowedWithoutAsking(request, fromApp, sourceOf(plugin({})))).toBe(false);
    // allowing again from the other starts over
    expect(allowing(fromApp, sourceOf(plugin({})), "https://example.com").origins).toEqual(["https://example.com"]);
  });
});
