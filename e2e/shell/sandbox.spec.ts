// What a plugin view can and cannot do from inside its frame.
//
// The core's API tests assert the headers; this asserts what a browser
// actually does with them, which is the property people rely on: a review's
// payload is often code that has been pushed nowhere, and the person opening
// it installed the plugin once, months ago, from someone else's repository.
//
// The probes that must fail point at the app's own server rather than the
// internet, so a machine with no network cannot make them pass for the wrong
// reason: the address is live and the policy refuses it anyway. Where a
// refusal could also come from the network, the test reads the policy's own
// violation report and checks which rule fired.

import { expect, test, type Frame } from "@playwright/test";
import { clearInbox, core, corePort, createReview } from "./helpers";

/** Live, reachable, and outside every source the frame's policy allows. */
const refused = `${core}/api/v1/info`;
/** A font the app serves to plugin views, which the policy does allow. */
const sdkFont = `${core}/sdk/v1/files/inter-latin-wght-normal.woff2`;

/** Opens a review and hands back the plugin's frame. */
async function pluginFrame(page: import("@playwright/test").Page): Promise<Frame> {
  // a fresh review each time: the same one still pending would come back
  await clearInbox(page.request);
  const { id } = await createReview(page.request, {
    title: "sandbox",
    origin: { repo: "acme/api", workflow: "sandbox" },
  });
  await page.goto(`/#/reviews/${id}`);
  await expect(page.frameLocator("#plugin-frame").locator("body")).toBeVisible();
  // The frame is there before it is the plugin's: it starts blank and is
  // pointed at the view, so wait for the address rather than the element.
  let frame: Frame | undefined;
  await expect
    .poll(
      () => {
        frame = page.frames().find((f) => f.url().includes("/plugins/list/"));
        return Boolean(frame);
      },
      { message: "the plugin view never got a frame of its own" },
    )
    .toBe(true);
  return frame!;
}

test("a plugin view cannot reach the disk, the shell, or anything off its own frame", async ({ page }) => {
  const frame = await pluginFrame(page);

  const reach = await frame.evaluate(
    async (urls) => {
      // The policy reports every refusal it makes, naming the rule. Reading
      // these tells a failing test whether the policy did the blocking or
      // something else did.
      const rules: string[] = [];
      document.addEventListener("securitypolicyviolation", (e) => rules.push(e.violatedDirective));
      const tried = async (fn: () => unknown | Promise<unknown>) => {
        try {
          await fn();
          return "allowed";
        } catch (e) {
          return `refused: ${(e as Error).name}`;
        }
      };
      const out = {
        localStorage: await tried(() => localStorage.setItem("probe", "1")),
        indexedDB: await tried(() => indexedDB.open("probe")),
        caches: await tried(() => caches.open("probe")),
        filePicker: await tried(() =>
          (window as unknown as { showOpenFilePicker: () => Promise<unknown> }).showOpenFilePicker(),
        ),
        parentDom: await tried(() => parent.document.title),
        parentOrigin: await tried(() => parent.location.href),
        bridge: typeof (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__,
        fetch: await tried(() => fetch(urls.refused)),
        socket: await tried(() => new WebSocket(urls.refused.replace("http", "ws"))),
        rules: [] as string[],
      };
      await new Promise((r) => setTimeout(r, 300));
      out.rules = rules;
      return out;
    },
    { refused },
  );

  // Storage is where a frame would keep something between reviews: a list of
  // what it has seen, a payload saved for later, a copy of a key it found.
  // The frame has no same-origin token, so its document's origin is opaque
  // and every store refuses it. Nothing a plugin view does reaches the disk.
  expect(reach.localStorage, "localStorage writes to the profile on disk").toContain("refused");
  expect(reach.indexedDB, "IndexedDB would hold far more, on disk").toContain("refused");
  expect(reach.caches, "the cache store is disk too, and survives restarts").toContain("refused");

  // The one other way to the disk is asking the person for a file. The picker
  // refuses a sandboxed frame outright, so a plugin cannot put up a dialog
  // that looks like the app's own and read whatever is chosen.
  expect(reach.filePicker, "a file picker hands the frame a file off the disk").toContain("refused");

  // The shell holds every other review, the settings, and the controls that
  // decide this one. A frame that could read or drive it would not need a
  // network to do harm: it could approve on the person's behalf.
  expect(reach.parentDom, "the shell's DOM holds other reviews and the decision controls").toContain("refused");
  expect(reach.parentOrigin, "even the shell's address is off limits").toContain("refused");

  // The app's own bridge is what turns a call into a system action: install
  // the CLI, open a file, write the clipboard. It lives in the window, on an
  // origin the frame cannot touch, and must never appear inside a view.
  expect(reach.bridge, "the system-call bridge must not exist inside a view").toBe("undefined");

  // A view talks to the shell by posting messages and nothing else. Both of
  // these are refused by connect-src, including to the app's own server, so
  // a plugin cannot read the API directly or keep a socket open. The socket
  // is checked through the policy's report: a failed handshake would look
  // the same from the outside, and would prove nothing.
  expect(reach.fetch, "fetch is the obvious way to send a payload out").toContain("refused");
  expect(
    reach.rules.filter((r) => r.startsWith("connect-src")).length,
    "the policy refused both the fetch and the socket",
  ).toBeGreaterThanOrEqual(2);
});

test("a plugin view loads images and fonts from the app, and from nowhere else", async ({ page }) => {
  const frame = await pluginFrame(page);

  const load = await frame.evaluate(
    async (urls) => {
      const rules: string[] = [];
      document.addEventListener("securitypolicyviolation", (e) => rules.push(e.violatedDirective));
      const image = (src: string) =>
        new Promise<string>((resolve) => {
          const img = new Image();
          img.onload = () => resolve("loaded");
          img.onerror = () => resolve("blocked");
          img.src = src;
          setTimeout(() => resolve("blocked"), 3000);
        });
      const font = async (src: string) => {
        try {
          await new FontFace("probe", `url(${src})`).load();
          return "loaded";
        } catch {
          return "blocked";
        }
      };
      const out = {
        imageOff: await image(`${urls.refused}?leak=secret`),
        // the plugin's own icon, in its folder above the view
        imageFromPlugin: await image(new URL("../icon.svg", document.baseURI).href),
        fontOff: await font(urls.refused),
        fontFromSdk: await font(urls.sdkFont),
        rules: [] as string[],
      };
      await new Promise((r) => setTimeout(r, 300));
      out.rules = rules;
      return out;
    },
    { refused, core, sdkFont },
  );

  // The address of a request is a message. A rule in a stylesheet that the
  // agent copied in with someone else's content can spell a value out one
  // guess at a time, each guess a different image address, with no script
  // involved. That is why images are pinned to the app's own paths rather
  // than merely to hosts a plugin declares.
  expect(load.imageOff, "an image address carries whatever is put in it").toBe("blocked");
  expect(load.fontOff, "a font address carries data the same way").toBe("blocked");
  expect(
    load.rules.some((r) => r.startsWith("img-src")),
    "the policy refused the image, not the network",
  ).toBe(true);
  expect(
    load.rules.some((r) => r.startsWith("font-src")),
    "the policy refused the font, not the network",
  ).toBe(true);

  // The other half: the app serves the icon set and the typeface it draws
  // itself in, so a view looks like the window around it without reaching
  // outside. If either of these breaks, panels silently fall back.
  expect(load.imageFromPlugin, "a plugin's own icons must load").toBe("loaded");
  expect(load.fontFromSdk, "the typeface the app serves must load").toBe("loaded");
});

test("a view's own file opened as a page is as sandboxed as in its frame", async ({ page, context }) => {
  // The frame's sandbox attribute does nothing for a page loaded top-level,
  // and a view can get its own files loaded that way: the files sit on the
  // app's server, beside the API. Loaded on the API's origin, a page could
  // open a window on that origin without a policy and drive the API from it:
  // read every review, decide them, install a plugin that runs a build.
  const view = (await pluginFrame(page)).url();
  const escaped = await context.newPage();
  await escaped.goto(view);

  const reach = await escaped.evaluate(async (sdk) => {
    const opened = window.open(sdk);
    let window_: string;
    try {
      window_ = opened ? `allowed: ${opened.document.title}` : "refused: no window";
    } catch (e) {
      window_ = `refused: ${(e as Error).name}`;
    }
    opened?.close();
    return { origin: self.origin, window: window_ };
  }, `${core}/sdk/v1/pinrail-plugin.js`);

  expect.soft(reach.origin, "the page must not have the app's origin").toBe("null");
  expect.soft(reach.window, "a window on the app's origin is one the page could drive").toContain("refused");
});

test("a view cannot have the shell open the app's own server", async ({ page }) => {
  // Opening a link is the one thing a view asks the shell to do with the
  // world outside. Pointed at the app's own server, it would load a view's
  // file top-level, or the API, outside every frame.
  const frame = await pluginFrame(page);
  // Outside the app the shell opens a link with window.open, and the browser
  // blocks a window no click started; record what the shell asks for instead
  await page.evaluate(() => {
    const opened: string[] = [];
    (window as unknown as { opened: string[] }).opened = opened;
    window.open = (url?: string | URL) => {
      opened.push(String(url));
      return null;
    };
  });
  const opened = () => page.evaluate(() => (window as unknown as { opened: string[] }).opened);

  const ask = (url: string) =>
    frame.evaluate((u) => parent.postMessage({ pinrail: 1, type: "open", url: u }, "*"), url);
  await ask(frame.url());
  await ask(`${core}/api/v1/info`);
  await ask(`http://localhost:${corePort}/api/v1/info`);
  // the control: a link out is asked about and opens, so the refusals above
  // are the rule's; the app's own addresses never reach the question
  await ask("https://example.invalid/");
  const dialog = page.locator("[data-link-dialog]");
  await expect(dialog.locator("[data-link-target]")).toHaveText("example.invalid");
  await dialog.locator("[data-link-once]").click();

  await expect
    .poll(async () => (await opened()).length, { message: "the link out never opened" })
    .toBeGreaterThanOrEqual(1);
  expect(await opened(), "the shell opened the app's own server").toEqual(["https://example.invalid/"]);
});
