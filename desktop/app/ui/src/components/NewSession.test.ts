import { describe, expect, it } from "vitest";
import { baseName } from "./NewSession";

describe("a folder's name", () => {
  it("is its last part, with either separator and without a trailing one", () => {
    expect(baseName("/Users/ann/code/pinrail/")).toBe("pinrail");
    expect(baseName(String.raw`C:\Users\ann\code\pinrail\ `.trim())).toBe("pinrail");
    expect(baseName("C:/Users/ann/code/app")).toBe("app");
  });
});
