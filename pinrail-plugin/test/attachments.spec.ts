import { expect, test } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { fixture, mountPlugin } from "@forgeplane/pinrail-plugin/testing";
import { scratch } from "./scratch.cjs";

// Files a review carries, in the place a view uses them: a frame that can
// fetch nothing, which asks the shell by name and gets the bytes back.

/** A plugin whose view asks for the file its payload names and reports what came. */
function reader(): string {
  const dir = scratch("pinrail-attachments-");
  fs.writeFileSync(
    path.join(dir, "manifest.json"),
    JSON.stringify({
      name: "reader",
      version: "1.0.0",
      title: "Reader",
      entry: "index.html",
      attachments: { accept: [".bin", "image/*"] },
    }),
  );
  fs.writeFileSync(
    path.join(dir, "index.html"),
    `<!doctype html>
<meta charset="utf-8">
<script src="/sdk/v1/pinrail-plugin.js"></script>
<pre id="out">waiting</pre>
<img id="img">
<script>
  const plugin = Pinrail.connect({
    async onInit({ review }) {
      const out = document.getElementById("out");
      try {
        const name = Pinrail.attachmentName(review.payload.file);
        const bytes = new Uint8Array(await plugin.attachment(name));
        let sum = 0;
        for (const b of bytes) sum = (sum + b) % 65521;
        out.textContent = name + " " + bytes.length + " bytes, sum " + sum + ", listed " + plugin.attachments.map((a) => a.name).join(",");
        if (review.payload.image) document.getElementById("img").src = await plugin.attachmentUrl(Pinrail.attachmentName(review.payload.image));
      } catch (e) {
        out.textContent = "refused: " + e.message;
      }
    },
  });
</script>`,
  );
  // every byte value, so nothing is decoded on the way
  const bytes = Buffer.from(Array.from({ length: 200_000 }, (_, i) => (i * 13) % 256));
  fs.writeFileSync(path.join(dir, "data.bin"), bytes);
  // a one-pixel PNG
  fs.writeFileSync(
    path.join(dir, "dot.png"),
    Buffer.from(
      "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
      "base64",
    ),
  );
  return dir;
}

const sum = (buf: Buffer) => buf.reduce((s, b) => (s + b) % 65521, 0);

test("a view gets a file's bytes from the shell, and an image as a blob: URL", async ({ page }) => {
  const dir = reader();
  const plugin = await mountPlugin(page, dir, {
    review: { title: "files", payload: { file: { $attachment: "data.bin" }, image: { $attachment: "dot.png" } } },
    attachments: { "data.bin": "data.bin", "dot.png": "dot.png" },
  });
  const expected = sum(fs.readFileSync(path.join(dir, "data.bin")));
  await expect(plugin.frame.locator("#out")).toHaveText(
    `data.bin 200000 bytes, sum ${expected}, listed data.bin,dot.png`,
  );
  await expect(plugin.frame.locator("#img")).toHaveAttribute("src", /^blob:/);
  await expect.poll(() => plugin.frame.locator("#img").evaluate((img: HTMLImageElement) => img.naturalWidth)).toBe(1);
});

test("a name the review does not list is refused without asking the shell", async ({ page }) => {
  const plugin = await mountPlugin(page, reader(), {
    review: { title: "files", payload: { file: { $attachment: "other.bin" } } },
    attachments: { "data.bin": "data.bin" },
  });
  await expect(plugin.frame.locator("#out")).toHaveText('refused: no attachment "other.bin" on this review');
  expect((await plugin.messages()).filter((m) => m.type === "attachment")).toEqual([]);
});

test("under an app too old to hand files over, the view is told so", async ({ page }) => {
  const plugin = await mountPlugin(page, reader(), {
    review: { title: "files", payload: { file: { $attachment: "data.bin" } } },
    attachments: { "data.bin": "data.bin" },
    capabilities: [],
  });
  await expect(plugin.frame.locator("#out")).toHaveText(
    "refused: this version of Pinrail cannot hand files to a view; update the app",
  );
});

test("a fixture lists its files by path, beside it", async ({ page }) => {
  const dir = reader();
  fs.mkdirSync(path.join(dir, "fixtures"));
  fs.copyFileSync(path.join(dir, "data.bin"), path.join(dir, "fixtures", "data.bin"));
  const file = path.join(dir, "fixtures", "files.json");
  fs.writeFileSync(
    file,
    JSON.stringify({
      title: "files",
      payload: { file: { $attachment: "data.bin" } },
      attachments: { "data.bin": { path: "data.bin" } },
    }),
  );
  const review = fixture(file);
  expect(review.attachments).toEqual([
    {
      name: "data.bin",
      size: 200000,
      media_type: "application/octet-stream",
      sha256: expect.stringMatching(/^[0-9a-f]{64}$/),
    },
  ]);
  const plugin = await mountPlugin(page, dir, { review });
  await expect(plugin.frame.locator("#out")).toContainText("data.bin 200000 bytes");
});
