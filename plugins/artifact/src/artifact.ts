// The artifact lives in a shadow root of the plugin's own document, so the
// view can point at its elements. Its styles come along and stay scoped;
// its scripts do not run; its links and forms go nowhere.

export type Box = { x: number; y: number; w: number; h: number };

export const BODY_CLASS = "artifact-body";

/** Parses the document and mounts it under `host`. Returns the body stand-in. */
export function mount(host: HTMLElement, html: string): { root: ShadowRoot; body: HTMLElement } {
  const root = host.shadowRoot ?? host.attachShadow({ mode: "open" });
  root.replaceChildren();
  const doc = new DOMParser().parseFromString(html, "text/html");

  const base = document.createElement("style");
  base.textContent = `:host { display: block; contain: content; } .${BODY_CLASS} { position: relative; min-height: 100%; }`;
  root.append(base);

  // The artifact's stylesheets, with html and body selectors pointed at the
  // stand-in for the body. External sheets cannot load in the view.
  for (const style of doc.querySelectorAll("style")) {
    const scoped = document.createElement("style");
    scoped.textContent = rescope(style.textContent ?? "");
    root.append(scoped);
  }

  const body = document.createElement("div");
  body.className = BODY_CLASS;
  for (const { name, value } of Array.from(doc.body.attributes)) {
    if (name === "class") body.className = `${BODY_CLASS} ${value}`;
    else if (name === "style" || name.startsWith("data-")) body.setAttribute(name, value);
  }
  for (const attr of ["style", "class"]) {
    const value = doc.documentElement.getAttribute(attr);
    if (value && attr === "style") body.setAttribute("style", `${value};${body.getAttribute("style") ?? ""}`);
  }
  for (const script of doc.querySelectorAll("script")) script.remove();
  for (const link of doc.querySelectorAll("link")) link.remove();
  body.append(...Array.from(doc.body.childNodes).map((node) => document.adoptNode(node)));
  root.append(body);

  // links and forms stay put: a mockup is for looking at
  root.addEventListener("click", (event) => {
    const target = event.composedPath()[0] as Element | undefined;
    if (target && (target as Element).closest?.("a, button[type=submit], input[type=submit]")) event.preventDefault();
  });
  root.addEventListener("submit", (event) => event.preventDefault());
  return { root, body };
}

/**
 * `:root`, `html` and `body` in the artifact's CSS become the stand-in
 * element: custom properties declared there inherit down from it, as they
 * would from the document element.
 */
function rescope(css: string): string {
  return css
    .replace(/(^|[\s,}>~+]):root(?=[\s,{.#:[])/g, `$1.${BODY_CLASS}`)
    .replace(/(^|[\s,}>~+])html\s*,\s*body(?=[\s,{.#:[])/g, `$1.${BODY_CLASS}`)
    .replace(/(^|[\s,}>~+])body(?=[\s,{.#:[])/g, `$1.${BODY_CLASS}`)
    .replace(/(^|[\s,}>~+])html(?=[\s,{.#:[])/g, `$1.${BODY_CLASS}`);
}

/**
 * A CSS selector for `el`, unique inside the artifact and relative to its
 * body, the way DevTools copies one: the element's own id or test id when
 * that is enough, else a path of tags with `:nth-of-type` where siblings
 * share a tag, anchored at the nearest ancestor with an id, or as short as
 * it can be when there is none.
 */
export function selectorFor(el: Element, root: ShadowRoot, body: Element): string {
  const unique = (s: string) => {
    try {
      return root.querySelectorAll(s).length === 1;
    } catch {
      return false;
    }
  };
  const own = idSelector(el);
  if (own && unique(own)) return own;
  const testId = el.getAttribute("data-testid");
  if (testId && unique(`[data-testid="${CSS.escape(testId)}"]`)) return `[data-testid="${CSS.escape(testId)}"]`;

  const parts: string[] = [];
  let cur: Element | null = el;
  while (cur && cur !== body) {
    const parent: Element | null = cur.parentElement;
    if (cur !== el) {
      const anchor = idSelector(cur);
      if (anchor && unique(anchor)) return [anchor, ...parts].join(" > ");
    }
    let part = cur.localName;
    if (parent) {
      const same = Array.from(parent.children).filter((c) => c.localName === cur!.localName);
      if (same.length > 1) part += `:nth-of-type(${same.indexOf(cur) + 1})`;
    }
    parts.unshift(part);
    cur = parent;
  }
  // no id above it: the shortest tail of the path that is still unique
  for (let i = parts.length - 1; i >= 0; i--) {
    const candidate = parts.slice(i).join(" > ");
    if (unique(candidate)) return candidate;
  }
  return parts.join(" > ");
}

const idSelector = (el: Element) => (el.id ? `#${CSS.escape(el.id)}` : null);

/** The element's text, trimmed to one line. */
export function snippetOf(el: Element, max = 80): string {
  const text = (el.textContent ?? "").replace(/\s+/g, " ").trim();
  return text.length > max ? `${text.slice(0, max - 1)}…` : text;
}

/** The start of the element's markup, with its attributes. */
export function markupOf(el: Element, max = 240): string {
  const html = el.outerHTML.replace(/\s+/g, " ");
  return html.length > max ? `${html.slice(0, max - 1)}…` : html;
}

/** Where `el` sits inside `layer`, in the layer's own coordinates. */
export function boxIn(el: Element, layer: Element): Box {
  const a = el.getBoundingClientRect();
  const b = layer.getBoundingClientRect();
  return { x: a.left - b.left, y: a.top - b.top, w: a.width, h: a.height };
}

/** Something to comment on: not the body stand-in, not our own chrome. */
export function commentable(el: Element | null, body: Element): Element | null {
  if (!el || el === body || !body.contains(el)) return null;
  return el;
}
