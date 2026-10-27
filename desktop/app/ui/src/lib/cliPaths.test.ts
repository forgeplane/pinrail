import { describe, expect, test } from "vitest";
import { folder, tilde } from "./cliPaths";

describe("tilde", () => {
  test("puts ~ in place of the home folder on Unix", () => {
    const short = tilde("/home/u/.local/bin/pinrail");
    expect(short("/home/u/.local/bin/pinrail")).toBe("~/.local/bin/pinrail");
    expect(short("/usr/local/bin/pinrail")).toBe("/usr/local/bin/pinrail");
  });

  test("puts ~ in place of the home folder on Windows", () => {
    const short = tilde(String.raw`C:\Users\u\.local\bin\pinrail.exe`);
    expect(short(String.raw`C:\Users\u\.local\bin\pinrail.exe`)).toBe(String.raw`~\.local\bin\pinrail.exe`);
    expect(short(String.raw`C:\Users\u\tools\pinrail.cmd`)).toBe(String.raw`~\tools\pinrail.cmd`);
    // another account whose name starts with this one's
    expect(short(String.raw`C:\Users\uma\bin\pinrail.exe`)).toBe(String.raw`C:\Users\uma\bin\pinrail.exe`);
  });

  test("leaves every path as it is when the link is not in a home folder", () => {
    expect(tilde("/opt/pinrail")("/opt/pinrail")).toBe("/opt/pinrail");
  });
});

describe("folder", () => {
  test("is the CLI's folder, with either separator", () => {
    expect(folder("/home/u/.local/bin/pinrail")).toBe("/home/u/.local/bin");
    expect(folder(String.raw`C:\Users\u\.local\bin\pinrail.exe`)).toBe(String.raw`C:\Users\u\.local\bin`);
  });
});
