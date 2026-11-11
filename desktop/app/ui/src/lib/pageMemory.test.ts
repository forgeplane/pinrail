import { describe, expect, test } from "vitest";
import { pageLink, rememberPage } from "./pageMemory";

describe("pageMemory", () => {
  test("a main page comes back with the query it was left with, without its open panel", () => {
    rememberPage("/items", "?status=done&repo=acme%2Fapi&open=PIN-4");
    expect(pageLink("/items")).toEqual({ pathname: "/items", search: "?status=done&repo=acme%2Fapi" });
    // left without a filter: the bare page
    rememberPage("/items", "?open=PIN-4");
    expect(pageLink("/items")).toEqual({ pathname: "/items", search: "" });
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
