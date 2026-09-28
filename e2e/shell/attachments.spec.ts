import { expect, test } from "@playwright/test";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { core, linkPlugin } from "./helpers";
import { scratch } from "../helpers/scratch";


/** A plugin whose view asks for the file its payload names and reports what came. */
function reader(): string {
  const dir = scratch("pinrail-reader-");
  fs.writeFileSync(
    path.join(dir, "manifest.json"),
    JSON.stringify({ name: "reader", version: "1.0.0", title: "Reader", entry: "index.html", payload_schema: {}, decision_schema: {}, attachments: { accept: [".bin"] } }),
  );
  fs.writeFileSync(
    path.join(dir, "index.html"),
    `<!doctype html><meta charset="utf-8"><script src="/sdk/v1/pinrail-plugin.js"></script><pre id="out">waiting</pre>
<script>
  const plugin = Pinrail.connect({
    async onInit({ review }) {
      const out = document.getElementById("out");
      try {
        const first = new Uint8Array(await plugin.attachment(Pinrail.attachmentName(review.payload.file)));
        // asked again: a fresh copy, since the first was transferred
        const again = new Uint8Array(await plugin.attachment(Pinrail.attachmentName(review.payload.file)));
        const hex = (b) => Array.from(b.slice(0, 4), (x) => x.toString(16).padStart(2, "0")).join("");
        out.textContent = first.length + " bytes " + hex(first) + ", again " + again.length;
      } catch (e) {
        out.textContent = "refused: " + e.message;
      }
      try { await plugin.attachment("not-listed.bin"); } catch (e) { out.dataset.refusal = e.message; }
    },
  });
</script>`,
  );
  return dir;
}

test("a view gets the bytes of a file its review carries from the app, and only those", async ({ page }) => {
  await linkPlugin(page.request, reader(), "reader");

  const bytes = Buffer.concat([Buffer.from([0xca, 0xfe, 0xba, 0xbe]), crypto.randomBytes(500_000)]);
  const sha256 = crypto.createHash("sha256").update(bytes).digest("hex");
  const put = await page.request.put(`${core}/api/v1/attachments/${sha256}`, { headers: { "content-type": "application/octet-stream" }, data: bytes });
  expect(put.status(), await put.text()).toBe(201);
  const created = await page.request.post(`${core}/api/v1/reviews`, {
    data: {
      plugin: "reader",
      title: "A file for the view",
      requested_by: "spec",
      payload: { file: { $attachment: "data.bin" } },
      attachments: { "data.bin": { sha256, size: bytes.length, media_type: "application/octet-stream" } },
    },
  });
  expect(created.status(), await created.text()).toBe(201);
  const { id } = await created.json();

  // the inbox row says the review came with files, before it is opened
  await page.goto("/#/");
  const row = page.locator("[data-review-row]", { hasText: "A file for the view" });
  await expect(row.locator("[data-files-count]")).toHaveText("1");
  await expect(row.locator("[data-files-count]")).toHaveAttribute("aria-label", "1 file · 488 KB");
  await row.locator("[data-files-count]").hover();
  await expect(page.locator(".tooltip")).toHaveText("1 file · 488 KB");
  if (process.env.PINRAIL_SHOTS) await page.screenshot({ path: path.join(process.env.PINRAIL_SHOTS, "inbox.png"), clip: { x: 0, y: 0, width: 1280, height: 260 } });

  await page.goto(`/#/reviews/${id}`);
  const out = page.frameLocator("#plugin-frame").locator("#out");
  await expect(out).toHaveText(`${bytes.length} bytes cafebabe, again ${bytes.length}`);
  await expect(out).toHaveAttribute("data-refusal", 'no attachment "not-listed.bin" on this review');

  // the strip says what came with the review, for every plugin alike
  const chip = page.locator("[data-attachments-chip]");
  await expect(chip).toHaveText("1 file · 488 KB");
  if (process.env.PINRAIL_SHOTS) await page.screenshot({ path: path.join(process.env.PINRAIL_SHOTS, "strip.png"), clip: { x: 0, y: 0, width: 1280, height: 160 } });
  await chip.click();
  const panel = page.locator("[data-attachments-panel]");
  await expect(panel.locator("[data-attachment]")).toHaveText([new RegExp(`data\\.bin.*488 KB · application/octet-stream · ${sha256.slice(0, 12)}`)]);
  if (process.env.PINRAIL_SHOTS) await page.screenshot({ path: path.join(process.env.PINRAIL_SHOTS, "panel.png"), clip: { x: 0, y: 0, width: 1280, height: 320 } });
  // outside the app, Save… is the core's attachment, downloaded
  const download = page.waitForEvent("download");
  await panel.locator('[data-attachment-save="data.bin"]').click();
  const saved = await (await download).path();
  expect(fs.readFileSync(saved!).equals(bytes)).toBe(true);
  await page.keyboard.press("Escape");
  await expect(panel).toHaveCount(0);

  await page.request.post(`${core}/api/v1/reviews/${id}/discard`, { data: { reason: "spec cleanup" } });
  // and so does its row in History once it has ended
  await page.goto("/#/history");
  await expect(page.locator("[data-history-row]", { hasText: "A file for the view" }).locator("[data-files-count]")).toHaveText("1");
  await page.request.delete(`${core}/api/v1/plugins/reader`);
});

test("a plugin that takes files says so on its row, and Settings › Data totals the files kept", async ({ page }) => {
  await linkPlugin(page.request, reader(), "reader");
  const info = await (await page.request.get(`${core}/api/v1/info`)).json();

  await page.goto("/#/");
  await page.keyboard.press("ControlOrMeta+,");
  await page.locator('[data-section="plugins"]').click();
  await expect(page.locator('[data-plugin-row="reader"] [data-plugin-takes]')).toHaveText("takes files: .bin");
  await page.locator('[data-section="data"]').click();
  await expect(page.locator("[data-attachment-totals]")).toHaveText(info.attachments.count === 0 ? "None stored" : /\d+ files?, .+\. They go with their reviews/);

  await page.request.delete(`${core}/api/v1/plugins/reader`);
});
