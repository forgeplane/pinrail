// Runs inside the view the development shell serves, added by the shell to
// the view's page, so that a person can pick a part of the view to comment
// on. While the shell has Select on, the element under the pointer is
// outlined and named, and a click picks it instead of reaching the view;
// the shell is told the element's selector, text and position. The shell
// also has it mark the elements already commented on with numbered pins.
// Its messages carry `pinrailReview`, apart from the app's protocol.
(() => {
  const KEY = "pinrailReview";
  let selecting = false;
  let pins = [];
  // the key that turns Select on and off, unless the plugin uses it itself
  let hotkey = null;
  const post = (msg) => parent.postMessage({ [KEY]: 1, ...msg }, "*");

  const layer = document.createElement("div");
  layer.setAttribute("data-pinrail-review", "");
  layer.style.cssText = "position:fixed;inset:0;pointer-events:none;z-index:2147483647";
  const box = document.createElement("div");
  box.style.cssText =
    "position:fixed;display:none;outline:2px solid #e5694f;outline-offset:1px;border-radius:3px;background:rgba(229,105,79,.08)";
  const label = document.createElement("div");
  label.style.cssText =
    "position:fixed;display:none;background:#e5694f;color:#fff;font:11px/1.6 ui-monospace,Menlo,monospace;padding:0 6px;border-radius:3px;white-space:nowrap;max-width:60vw;overflow:hidden;text-overflow:ellipsis";
  const marks = document.createElement("div");
  layer.append(marks, box, label);
  const mount = () => document.documentElement.append(layer);
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", mount);
  else mount();

  const ours = (el) => !!(el && el.closest && el.closest("[data-pinrail-review]"));
  const text = (el) => (el.innerText || el.textContent || "").trim().replace(/\s+/g, " ").slice(0, 60);
  const unique = (s) => {
    try {
      return document.querySelectorAll(s).length === 1;
    } catch {
      return false;
    }
  };
  const idOf = (el) => (el.id ? `#${CSS.escape(el.id)}` : null);

  // A selector for `el`, unique in the view, the way DevTools copies one:
  // its own id or test id when that is enough, else a path of tags with
  // :nth-of-type where siblings share a tag, anchored at the nearest
  // ancestor with an id, or the shortest unique tail of that path.
  function selectorFor(el) {
    const own = idOf(el);
    if (own && unique(own)) return own;
    const testId = el.getAttribute("data-testid");
    if (testId && unique(`[data-testid="${CSS.escape(testId)}"]`)) return `[data-testid="${CSS.escape(testId)}"]`;
    const parts = [];
    for (let cur = el; cur && cur !== document.documentElement; cur = cur.parentElement) {
      if (cur !== el) {
        const anchor = idOf(cur);
        if (anchor && unique(anchor)) return [anchor, ...parts].join(" > ");
      }
      let part = cur.localName;
      const parent = cur.parentElement;
      if (parent) {
        const same = Array.from(parent.children).filter((c) => c.localName === cur.localName);
        if (same.length > 1) part += `:nth-of-type(${same.indexOf(cur) + 1})`;
      }
      parts.unshift(part);
    }
    for (let i = parts.length - 1; i >= 0; i--) {
      const candidate = parts.slice(i).join(" > ");
      if (unique(candidate)) return candidate;
    }
    return parts.join(" > ");
  }

  const name = (el) => {
    const t = text(el);
    return `${el.localName}${el.classList[0] ? "." + el.classList[0] : ""}${t ? ` "${t}"` : ""}`;
  };

  function outline(el) {
    if (!el || ours(el) || el === document.documentElement || el === document.body) {
      box.style.display = label.style.display = "none";
      return;
    }
    const r = el.getBoundingClientRect();
    Object.assign(box.style, {
      display: "block",
      left: r.left + "px",
      top: r.top + "px",
      width: r.width + "px",
      height: r.height + "px",
    });
    label.textContent = name(el);
    Object.assign(label.style, {
      display: "block",
      left: Math.max(0, r.left) + "px",
      top: Math.max(0, r.top - 18) + "px",
    });
  }

  function drawPins() {
    marks.replaceChildren();
    for (const pin of pins) {
      let el = null;
      try {
        el = document.querySelector(pin.selector);
      } catch {
        // a selector the page no longer matches draws no pin
      }
      if (!el) continue;
      const r = el.getBoundingClientRect();
      const mark = document.createElement("div");
      mark.setAttribute("data-pinrail-review-pin", "");
      mark.textContent = String(pin.n);
      mark.style.cssText = `position:fixed;left:${Math.max(0, r.left - 9)}px;top:${Math.max(0, r.top - 9)}px;width:18px;height:18px;border-radius:50%;background:#e5694f;color:#fff;font:600 10px/18px system-ui,sans-serif;text-align:center;box-shadow:0 1px 3px rgba(0,0,0,.4)`;
      marks.append(mark);
    }
  }
  setInterval(() => pins.length && drawPins(), 400);
  addEventListener("scroll", () => pins.length && drawPins(), true);

  addEventListener(
    "pointermove",
    (e) => {
      if (selecting) outline(document.elementFromPoint(e.clientX, e.clientY));
    },
    true,
  );
  // while selecting, the view sees no press or click
  for (const type of ["pointerdown", "pointerup", "mousedown", "mouseup", "click", "dblclick", "auxclick"]) {
    addEventListener(
      type,
      (e) => {
        if (!selecting) return;
        e.preventDefault();
        e.stopImmediatePropagation();
        if (type !== "click") return;
        const el = document.elementFromPoint(e.clientX, e.clientY);
        if (!el || ours(el) || el === document.documentElement || el === document.body) return;
        const r = el.getBoundingClientRect();
        post({
          type: "pick",
          selector: selectorFor(el),
          tag: el.localName,
          text: text(el),
          rect: { left: r.left, top: r.top, right: r.right, bottom: r.bottom },
        });
      },
      true,
    );
  }
  addEventListener(
    "keydown",
    (e) => {
      const t = e.target;
      if (t && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName))) return;
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      if ((hotkey && e.key === hotkey) || (e.key === "Escape" && selecting)) {
        e.preventDefault();
        e.stopImmediatePropagation();
        post({ type: "key", key: e.key });
      }
    },
    true,
  );

  addEventListener("message", (e) => {
    if (e.source !== parent || !e.data || !e.data[KEY]) return;
    if (e.data.type === "select") {
      selecting = !!e.data.on;
      hotkey = e.data.key || null;
      document.documentElement.style.cursor = selecting ? "crosshair" : "";
      if (!selecting) outline(null);
    }
    if (e.data.type === "pins") {
      pins = e.data.pins || [];
      drawPins();
    }
  });
  post({ type: "loaded" });
})();
