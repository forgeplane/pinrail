import { describe, expect, test } from "vitest";
import { age, settledAt, size, stamp, takes } from "./format";

const now = Date.parse("2026-09-28T12:00:00Z");
const ago = (seconds: number) => new Date(now - seconds * 1000).toISOString();

describe("age", () => {
  test("is said in the largest whole unit, up to days", () => {
    expect(age(ago(0), now)).toBe("0s");
    expect(age(ago(59), now)).toBe("59s");
    expect(age(ago(60), now)).toBe("1m");
    expect(age(ago(59 * 60 + 59), now)).toBe("59m");
    expect(age(ago(60 * 60), now)).toBe("1h");
    expect(age(ago(47 * 3600), now)).toBe("47h");
    expect(age(ago(48 * 3600), now)).toBe("2d");
  });

  test("of a moment still to come is 0s, and of nothing is empty", () => {
    expect(age(ago(-30), now)).toBe("0s");
    expect(age(null, now)).toBe("");
    expect(age(undefined, now)).toBe("");
  });
});

describe("size", () => {
  test("names bytes, then KB, MB and GB", () => {
    expect(size(1)).toBe("1 byte");
    expect(size(12)).toBe("12 bytes");
    expect(size(1023)).toBe("1023 bytes");
    expect(size(1024)).toBe("1 KB");
    expect(size(1536)).toBe("2 KB");
    expect(size(1024 * 1024)).toBe("1.0 MB");
    expect(size(10 * 1024 * 1024)).toBe("10.0 MB");
    expect(size(1024 * 1024 * 1024)).toBe("1.0 GB");
  });
});

describe("takes", () => {
  test("lists the kinds, then the limits that are set", () => {
    expect(takes({ accept: [".glb", ".gltf"] })).toBe("Takes files: .glb, .gltf");
    expect(takes({ accept: [".pdf"], max_size: 50 * 1024 * 1024, max_count: 3 })).toBe(
      "Takes files: .pdf, up to 50.0 MB each, 3 at most",
    );
    expect(takes({ accept: [".html"], max_count: 1 })).toBe("Takes files: .html, 1 at most");
  });
});

describe("settledAt", () => {
  const ended = { decision: null, withdrawn_at: null, discarded_at: null, expires_at: null };

  test("is when the review was decided, withdrawn, discarded or expired", () => {
    const decided = { decided_by: "pat", decided_at: "2026-09-28T10:00:00Z", data: {} };
    expect(settledAt({ ...ended, decision: decided as never })).toBe("2026-09-28T10:00:00Z");
    expect(settledAt({ ...ended, withdrawn_at: "2026-09-28T09:00:00Z" })).toBe("2026-09-28T09:00:00Z");
    expect(settledAt({ ...ended, discarded_at: "2026-09-28T08:00:00Z" })).toBe("2026-09-28T08:00:00Z");
    expect(settledAt({ ...ended, expires_at: "2026-09-28T07:00:00Z" })).toBe("2026-09-28T07:00:00Z");
  });

  test("of a discarded review that also had an expiry is when it was discarded", () => {
    expect(settledAt({ ...ended, discarded_at: "2026-09-28T08:00:00Z", expires_at: "2026-09-29T00:00:00Z" })).toBe(
      "2026-09-28T08:00:00Z",
    );
  });
});

describe("stamp", () => {
  test("leaves what is not a date as it is, and nothing empty", () => {
    expect(stamp("not a date")).toBe("not a date");
    expect(stamp(null)).toBe("");
  });
});
