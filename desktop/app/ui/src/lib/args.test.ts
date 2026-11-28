import { describe, expect, test } from "vitest";
import { splitArgs } from "./args";

describe("extra arguments", () => {
  test("are split on spaces", () => {
    expect(splitArgs("  --model  opus --verbose ", false)).toEqual(["--model", "opus", "--verbose"]);
  });
  test("keep quoted spaces, and empty quotes as an empty word", () => {
    expect(splitArgs(`--name "fix the login" --tag 'a b' ""`, false)).toEqual([
      "--name",
      "fix the login",
      "--tag",
      "a b",
      "",
    ]);
  });
  test("keep a character after a backslash", () => {
    expect(splitArgs(String.raw`a\ b "say \"hi\""`, false)).toEqual(["a b", 'say "hi"']);
  });
  test("of an empty field are none", () => {
    expect(splitArgs("   ", false)).toEqual([]);
  });
});

describe("extra arguments on Windows", () => {
  test("keep a folder's backslashes, and quoted spaces", () => {
    expect(splitArgs(String.raw`--add-dir C:\code\app --model "opus 4" ""`, true)).toEqual([
      "--add-dir",
      String.raw`C:\code\app`,
      "--model",
      "opus 4",
      "",
    ]);
  });
  test("take a backslash before a quote as Windows programs do", () => {
    expect(splitArgs(String.raw`a\"b "c\\" "say ""hi""" d\\\"e`, true)).toEqual(['a"b', "c\\", 'say "hi"', 'd\\"e']);
  });
  test("know only double quotes", () => {
    expect(splitArgs("--tag 'a b'", true)).toEqual(["--tag", "'a", "b'"]);
  });
});
