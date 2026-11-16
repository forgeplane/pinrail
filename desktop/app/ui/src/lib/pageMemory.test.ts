import { describe, expect, test } from "vitest";
import { pageLink, rememberedScroll, rememberPage, rememberScroll } from "./pageMemory";

describe("pageMemory", () => {
  test("a main page comes back with the query it was left with, its open panel included", () => {
    rememberPage("/items", "?status=done&repo=acme%2Fapi&open=PIN-4");
    expect(pageLink("/items")).toEqual({ pathname: "/items", search: "?status=done&repo=acme%2Fapi&open=PIN-4" });
    // left without a filter: the bare page
    rememberPage("/items", "");
    expect(pageLink("/items")).toEqual({ pathname: "/items", search: "" });
  });

  test("a main page's scroll position is kept by its address", () => {
    rememberScroll("/items", "?repo=acme%2Fapi", 480);
    expect(rememberedScroll("/items", "?repo=acme%2Fapi")).toBe(480);
    expect(rememberedScroll("/items", "?repo=acme%2Fweb")).toBeUndefined();
    rememberScroll("/reviews/r_1", "", 200);
    expect(rememberedScroll("/reviews/r_1", "")).toBeUndefined();
  });

  test("the inbox comes back on its own tab, with its filters", () => {
    rememberPage("/", "?tab=history&repo=acme%2Fapi");
    expect(pageLink("/")).toEqual({ pathname: "/", search: "?repo=acme%2Fapi" });
  });

  test("only the main pages are kept", () => {
    rememberPage("/reviews/r_1", "?x=1");
    expect(pageLink("/reviews/r_1")).toEqual({ pathname: "/reviews/r_1", search: "" });
    expect(pageLink("/sessions")).toEqual({ pathname: "/sessions", search: "" });
  });
});
