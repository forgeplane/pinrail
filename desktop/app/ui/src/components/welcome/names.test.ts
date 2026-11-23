import { describe, expect, it } from "vitest";
import { shortPath } from "./names";

describe("shortPath", () => {
  it("shows a short path whole, with the home folder as ~", () => {
    expect(shortPath("/Users/ann/code/pinrail")).toBe("~/code/pinrail");
  });

  it("keeps the start and the last two folders of a long path", () => {
    expect(
      shortPath("/private/tmp/claude-501/-Users-ann-code-pinrail/c942bfdf-3b0f-4bd1-8d17-6d2f72dd722b/scratchpad/rs-a"),
    ).toBe("/private/…/scratchpad/rs-a");
    expect(shortPath("/Users/ann/conductor/workspaces/worktreee/minnetonka/packages/web")).toBe("~/…/packages/web");
  });
});
