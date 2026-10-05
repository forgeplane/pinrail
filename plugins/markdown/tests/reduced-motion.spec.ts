import { expect, test } from "@playwright/test";
import path from "node:path";
import { fixture, mountPlugin } from "@forgeplane/pinrail-plugin/testing";

// With the system set to reduce motion, as many people have it, a diagram
// still draws at its size.
const dir = path.resolve(__dirname, "..");
test.use({ reducedMotion: "reduce" });

test("a diagram draws at its size with reduced motion", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: fixture(path.join(dir, "fixtures", "retries.json")) });
  const diagram = plugin.frame.locator(".diagram svg");
  await expect(diagram).toBeVisible({ timeout: 15_000 });
  await expect.poll(async () => (await diagram.boundingBox())!.height).toBeLessThan(500);
});
