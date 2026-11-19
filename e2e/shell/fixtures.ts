// Playwright for the shell specs, with these changes: every page of the
// browser is given this worker's core and the person token before it loads,
// as the app's window is given them by Tauri; what a spec sends the core
// through `request` or `page.request` acts as the person, with the person
// token, as the app's window does. The browser's own requests carry no
// token: the shell adds it itself, and a plugin view's frame must not.

import { test as base } from "@playwright/test";
import { asPerson, core, personToken } from "./helpers";

export * from "@playwright/test";

export const test = base.extend({
  context: async ({ context }, use) => {
    // the app's window only: a plugin view's frame never sees the token
    await context.addInitScript(
      (server) => {
        if (window === window.top) Object.assign(window, { __PINRAIL_SERVER__: server });
      },
      { url: core, token: personToken },
    );
    await use(context);
  },
  request: async ({ playwright }, use) => {
    const request = await playwright.request.newContext({ extraHTTPHeaders: asPerson });
    await use(request);
    await request.dispose();
  },
  page: async ({ page, playwright }, use) => {
    const request = await playwright.request.newContext({ extraHTTPHeaders: asPerson });
    Object.defineProperty(page, "request", { get: () => request });
    // the app renders after an awaited call and listens for keys from its
    // effects: going to one of its pages waits until it does, so that a
    // key pressed next is not lost
    const goto = page.goto.bind(page);
    page.goto = async (url, options) => {
      const response = await goto(url, options);
      if (url === "/" || url.startsWith("/#")) await page.locator("html[data-ready]").waitFor({ state: "attached" });
      return response;
    };
    await use(page);
    await request.dispose();
  },
});
