// Uncaught errors of the window, kept by the core as a crash report while
// crash reports are on, and sent at the next start. The core removes
// paths, quoted text, links and secrets before it keeps one.

import { api } from "../api/client";

// the core keeps only the first report until it is sent; once a window
// load is enough
let reported = false;

/** Errors the browser raises that break nothing. */
const HARMLESS = [/ResizeObserver loop/];

/** Reports an error that nothing caught. */
export function reportCrash(error: unknown) {
  const err = error instanceof Error ? error : new Error(String(error));
  if (reported || HARMLESS.some((pattern) => pattern.test(err.message))) return;
  reported = true;
  api.reportCrash(err.message || err.name, err.stack ?? "").catch(() => {});
}

/** Reports the errors that reach the window uncaught from now on. */
export function catchWindowErrors() {
  window.addEventListener("error", (event) => reportCrash(event.error ?? event.message));
  window.addEventListener("unhandledrejection", (event) => reportCrash(event.reason));
}
