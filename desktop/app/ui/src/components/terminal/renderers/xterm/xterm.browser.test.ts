// The xterm.js adapter, with xterm.js itself, in WebKit.
import { expect, test } from "vitest";
import type { InputKind } from "../../view";
import { xtermRenderer } from "./index";

test("text an input method commits is the person's input, not a reply", async () => {
  const host = document.createElement("div");
  host.style.cssText = "width: 400px; height: 200px; position: relative";
  document.body.append(host);
  const view = xtermRenderer.create();
  await view.mount(host, {
    cols: 40,
    rows: 10,
    scrollback: 100,
    theme: { foreground: "#e6e6e6", background: "#101010", cursor: "#e6e6e6" },
    font: { family: "monospace", size: 13 },
  });
  const inputs: [InputKind, string][] = [];
  const replies: string[] = [];
  view.onInput((bytes, kind) => inputs.push([kind, new TextDecoder().decode(bytes)]));
  view.onReply((bytes) => replies.push(new TextDecoder().decode(bytes)));
  view.focus();
  const textarea = host.querySelector("textarea")!;
  // as WebKit delivers a Japanese commit: the marked text in the textarea,
  // then compositionend with the text
  textarea.dispatchEvent(new CompositionEvent("compositionstart", { data: "", bubbles: true }));
  textarea.value = "日本";
  textarea.dispatchEvent(new CompositionEvent("compositionupdate", { data: "日本", bubbles: true }));
  textarea.dispatchEvent(new CompositionEvent("compositionend", { data: "日本", bubbles: true }));
  await new Promise((r) => setTimeout(r, 50));
  expect(replies).toEqual([]);
  expect(inputs).toEqual([["key", "日本"]]);
  view.dispose();
  host.remove();
});
