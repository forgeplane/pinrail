// A fake core for component tests: the API client's methods answer what a
// test sets, and the event stream delivers the notices a test sends. A
// test file replaces the client with it:
//
//   vi.mock("../api/client", async (real) =>
//     (await import("../testing/core")).fakeClient(await real()),
//   );
//
// Then `core.answer("items", { items: [...] })` sets what `api.items()`
// resolves to, `core.calls("patchItem")` lists what a component sent, and
// `core.notice({ kind: "item_changed", ... })` delivers a notice. A method
// with no answer never resolves, so a component stays loading rather than
// failing on an answer the test did not mean to give.

import { vi, type Mock } from "vitest";
import type * as Client from "../api/client";
import type { Notice } from "../api/types";

type Api = typeof Client.api;
type Method = keyof Api;
type Answer = unknown | ((...args: unknown[]) => unknown);

const answers = new Map<string, Answer>();
const mocks = new Map<string, Mock>();
const streams = new Set<{ onNotice: (notice: Notice) => void; onOpen?: () => void }>();

const mockOf = (name: string): Mock => {
  let mock = mocks.get(name);
  if (!mock) {
    mock = vi.fn(async (...args: unknown[]) => {
      if (!answers.has(name)) return new Promise(() => {});
      const answer = answers.get(name);
      return typeof answer === "function" ? answer(...args) : structuredClone(answer);
    });
    mocks.set(name, mock);
  }
  return mock;
};

/** Every method of the API client, each a mock answered from `answers`. */
const api = new Proxy({} as Api, { get: (_, name) => mockOf(String(name)) });

export const core = {
  /** Sets what `api[method]()` resolves to: a value, copied for each call,
   *  or a function of the call's arguments. */
  answer(method: Method, answer: Answer) {
    answers.set(method, answer);
  },
  /** The arguments of each call of `api[method]` so far. */
  calls(method: Method): unknown[][] {
    return mockOf(method).mock.calls;
  },
  /** Delivers a notice to every open event stream. */
  notice(notice: Notice) {
    for (const stream of streams) stream.onNotice(notice);
  },
  /** Forgets the answers and calls, between tests. */
  reset() {
    answers.clear();
    for (const mock of mocks.values()) mock.mockClear();
  },
};

/** The client module with the fake API and event stream, keeping the rest
 *  of the real one: its types, errors and helpers. The stream opens at once. */
export function fakeClient(real: typeof Client): typeof Client {
  return {
    ...real,
    api,
    inTauri: () => false,
    subscribe(handlers) {
      streams.add(handlers);
      queueMicrotask(() => handlers.onOpen?.());
      return () => streams.delete(handlers);
    },
  };
}
