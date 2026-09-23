import { expect, test } from "@playwright/test";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const core = "http://127.0.0.1:4799";

/** A plugin whose view asks for the file its payload names and reports what came. */
function reader(): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pinrail-reader-"));
  fs.writeFileSync(
    path.join(dir, "manifest.json"),
    JSON.stringify({ name: "reader", version: "1.0.0", title: "Reader", entry: "index.html", payload_schema: {}, decision_schema: {}, artifacts: { accept: [".bin"] } }),
  );
  fs.writeFileSync(
    path.join(dir, "index.html"),
    `<!doctype html><meta charset="utf-8"><script src="/sdk/v1/pinrail-plugin.js"></script><pre id="out">waiting</pre>
<script>
  const plugin = Pinrail.connect({
    async onInit({ gate }) {
      const out = document.getElementById("out");
      try {
        const first = new Uint8Array(await plugin.artifact(Pinrail.artifactName(gate.payload.file)));
        // asked again: a fresh copy, since the first was transferred
        const again = new Uint8Array(await plugin.artifact(Pinrail.artifactName(gate.payload.file)));
        const hex = (b) => Array.from(b.slice(0, 4), (x) => x.toString(16).padStart(2, "0")).join("");
        out.textContent = first.length + " bytes " + hex(first) + ", again " + again.length;
      } catch (e) {
        out.textContent = "refused: " + e.message;
      }
      try { await plugin.artifact("not-listed.bin"); } catch (e) { out.dataset.refusal = e.message; }
    },
  });
</script>`,
  );
  return dir;
}

test("a view gets the bytes of a file its review carries from the app, and only those", async ({ page }) => {
  const installed = await page.request.post(`${core}/api/v1/plugins/install`, { data: { source: reader(), link: true } });
  expect(installed.status(), await installed.text()).toBe(202);
  await expect
    .poll(async () => ((await (await page.request.get(`${core}/api/v1/plugins`)).json()).plugins as { name: string; usable: boolean }[]).some((p) => p.name === "reader" && p.usable))
    .toBe(true);

  const bytes = Buffer.concat([Buffer.from([0xca, 0xfe, 0xba, 0xbe]), crypto.randomBytes(500_000)]);
  const sha256 = crypto.createHash("sha256").update(bytes).digest("hex");
  const put = await page.request.put(`${core}/api/v1/artifacts/${sha256}`, { headers: { "content-type": "application/octet-stream" }, data: bytes });
  expect(put.status(), await put.text()).toBe(201);
  const created = await page.request.post(`${core}/api/v1/reviews`, {
    data: {
      plugin: "reader",
      title: "A file for the view",
      requested_by: "spec",
      payload: { file: { $artifact: "data.bin" } },
      artifacts: { "data.bin": { sha256, size: bytes.length, media_type: "application/octet-stream" } },
    },
  });
  expect(created.status(), await created.text()).toBe(201);
  const { id } = await created.json();

  await page.goto(`/#/reviews/${id}`);
  const out = page.frameLocator("#plugin-frame").locator("#out");
  await expect(out).toHaveText(`${bytes.length} bytes cafebabe, again ${bytes.length}`);
  await expect(out).toHaveAttribute("data-refusal", 'no artifact "not-listed.bin" on this review');

  await page.request.post(`${core}/api/v1/reviews/${id}/discard`, { data: { reason: "spec cleanup" } });
  await page.request.delete(`${core}/api/v1/plugins/reader`);
});
