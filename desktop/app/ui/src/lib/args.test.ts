import { describe, expect, test } from "vitest";
import { splitArgs } from "./args";

describe("extra arguments", () => {
  test("are split on spaces", () => {
    expect(splitArgs("  --model  opus --verbose ")).toEqual(["--model", "opus", "--verbose"]);
  });
  test("keep quoted spaces, and empty quotes as an empty word", () => {
    expect(splitArgs(`--name "fix the login" --tag 'a b' ""`)).toEqual(["--name", "fix the login", "--tag", "a b", ""]);
  });
  test("keep a character after a backslash", () => {
    expect(splitArgs(String.raw`a\ b "say \"hi\""`)).toEqual(["a b", 'say "hi"']);
  });
  test("of an empty field are none", () => {
    expect(splitArgs("   ")).toEqual([]);
  });
});
