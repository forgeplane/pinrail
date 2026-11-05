import { describe, expect, test } from "vitest";
import { imageUrl, middleSquare } from "./projectImage";

describe("a project's image", () => {
  test("is cropped to its middle square", () => {
    expect(middleSquare(400, 300)).toEqual({ x: 50, y: 0, side: 300 });
    expect(middleSquare(300, 500)).toEqual({ x: 0, y: 100, side: 300 });
    expect(middleSquare(256, 256)).toEqual({ x: 0, y: 0, side: 256 });
  });

  test("is served at an address that changes with the image", () => {
    expect(imageUrl("http://127.0.0.1:4747", { id: "p_1", image: "ab12" })).toBe(
      "http://127.0.0.1:4747/api/v1/projects/p_1/image?v=ab12",
    );
    expect(imageUrl("http://127.0.0.1:4747", { id: "p_1", image: null })).toBeNull();
  });
});
