import { describe, expect, test } from "vitest";
import { followable } from "./native";

describe("followable", () => {
  test("opens links to the outside world", () => {
    expect(followable("https://example.com/page")).toBe(true);
    expect(followable("mailto:someone@example.com")).toBe(true);
  });

  test("refuses every address of this machine, however it is written", () => {
    for (const url of [
      "http://localhost:4747/",
      "http://app.localhost/",
      "http://127.0.0.1:4747/",
      "http://127.1/",
      "http://0x7f.0.0.1/",
      "http://0.0.0.0/",
      "http://[::1]:4747/",
      "http://[::ffff:127.0.0.1]:4747/",
      "http://[0:0:0:0:0:ffff:7f00:1]/",
    ]) {
      expect(followable(url), url).toBe(false);
    }
  });

  test("refuses addresses given as IPv6 literals, which a view has no need for", () => {
    expect(followable("https://[2001:db8::1]/")).toBe(false);
  });
});
