// Playwright for the shell specs, with one change: what a spec sends the
// core through `request` or `page.request` acts as the person, with the
// person token, as the app's window does. The browser's own requests carry
// no token: the shell adds it itself, and a plugin view's frame must not.

import { test as base } from "@playwright/test";
import { asPerson } from "./helpers";

export * from "@playwright/test";

export const test = base.extend({
  request: async ({ playwright }, use) => {
    const request = await playwright.request.newContext({ extraHTTPHeaders: asPerson });
    await use(request);
    await request.dispose();
  },
  page: async ({ page, playwright }, use) => {
    const request = await playwright.request.newContext({ extraHTTPHeaders: asPerson });
    Object.defineProperty(page, "request", { get: () => request });
    await use(page);
    await request.dispose();
  },
});
