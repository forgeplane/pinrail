import { describe, expect, test } from "vitest";
import { inScope, scopeOf, scopeOptions, setScope } from "./url";

describe("the project or repository filter", () => {
  test("a project goes in ?project= and a repository in ?repo=, never both", () => {
    const params = new URLSearchParams("repo=acme%2Fapi&q=x");
    setScope(params, "project:p_1");
    expect(params.toString()).toBe("q=x&project=p_1");
    expect(scopeOf(params)).toBe("project:p_1");
    setScope(params, "acme/web");
    expect(params.toString()).toBe("q=x&repo=acme%2Fweb");
    setScope(params, "");
    expect(params.toString()).toBe("q=x");
  });

  test("a project admits its repositories' items, and - those with no repository", () => {
    const project = { id: "p_1" };
    expect(inScope("acme/api", project, "project:p_1")).toBe(true);
    expect(inScope("acme/api", null, "project:p_1")).toBe(false);
    expect(inScope("acme/api", project, "acme/api")).toBe(true);
    expect(inScope(null, null, "-")).toBe(true);
    expect(inScope("acme/api", null, "")).toBe(true);
  });

  test("the choices list every project, then the repositories, then none", () => {
    const options = scopeOptions([{ id: "p_1", name: "Pinrail" }], ["acme/api"], true, "");
    expect(options.map((o) => [o.value, o.label])).toEqual([
      ["", "All projects and repositories"],
      ["project:p_1", "Pinrail"],
      ["acme/api", "acme/api"],
      ["-", "No repository"],
    ]);
  });
});
