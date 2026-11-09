import { diffLines, diffWordsWithSpace } from "diff";
import MarkdownIt from "markdown-it";

let mermaidLoading = null;
/** Mermaid, imported the first time a document needs it. */
const loadMermaid = () => (mermaidLoading ??= import("mermaid").then((m) => m.default));

// A Markdown document under review, with its outline: rendered with its
// diagrams, or as its raw source. The person comments
// on a section, a diagram or a selected passage; every comment goes back as
// lines of the payload's markdown, with the headings it sits under.

const esc = Pinrail.escape;
const $ = (id) => document.getElementById(id);

// ------------------------------------------------------------------ state

let source = "";
let lines = [];
/** The document as the round this one revises had it, or null. */
let previousSource = null;
/** every heading: level, text, the line it starts on, its section's last line */
let headings = [];
let comments = [];
/** The side panel's tab, when the previous round has comments: this round's or the previous one's. */
let sideTab = "current";
let mode = "rendered";
/** what a new comment is about, while it is being written */
let composing = null;
/** the comment being edited */
let editing = null;
let renderGeneration = 0;
/** the outline beside the document: shown unless the person folded it away */
let outlineOpen = true;
/** the comments beside the document: shown unless the person folded them away */
let commentsOpen = true;
/** in a narrow frame, the outline or the comments open as a drawer over the document */
let drawer = null;
const narrowOutline = window.matchMedia("(max-width: 1100px)");
const narrowComments = window.matchMedia("(max-width: 760px)");
/** why the app refused the last hand-over */
let violations = [];

/* A comment is a change to make or a question to answer. */
const KINDS = { change: "Change", question: "Question" };
const PLACEHOLDERS = { change: "What should change here?", question: "What should the agent explain?" };
const kindOf = (comment) => (comment.kind === "question" ? "question" : "change");

/* What the hand-over needs, or why the app refused it: the SDK's
   confirmation bar, under the work area. */
const confirm = Pinrail.confirmationBar();
function syncBar() {
  confirm.confirmation(
    violations.length
      ? {
          tone: "danger",
          text: `The app refused the decision: ${violations.map((e) => `${e.path || "/"}: ${e.message}`).join("; ")}`,
        }
      : null,
  );
}

function applySettings(settings) {
  if (typeof settings?.outline_open === "boolean") outlineOpen = settings.outline_open;
  if (typeof settings?.comments_open === "boolean") commentsOpen = settings.comments_open;
}

const plugin = Pinrail.connect({
  onInit({ review, draft, settings }) {
    applySettings(settings);
    renderCommentsPanel();
    violations = [];
    const d = draft || {};
    comments = Array.isArray(d.comments) ? d.comments : [];
    mode = d.mode || "rendered";
    load(review);
  },
  onSubmitted() {
    closeDialog();
    renderSide();
    renderMarks();
  },
  onViolations(errors) {
    violations = errors;
    syncBar();
  },
  onSettings(settings) {
    applySettings(settings);
    renderOutline();
    renderCommentsPanel();
  },
  onCollect() {
    return handOver();
  },
  onAppearance() {
    renderDiagrams();
  },
});

// ------------------------------------------------------------------ the document

const md = new MarkdownIt({ html: false, linkify: true, typographer: false });

// every block keeps the lines of the source it came from, so a comment on
// anything rendered maps back to the markdown
md.core.ruler.push("source_lines", (state) => {
  for (const token of state.tokens) {
    if (token.map && token.nesting >= 0 && token.type !== "inline") {
      token.attrSet("data-line-start", String(token.map[0] + 1));
      token.attrSet("data-line-end", String(token.map[1]));
    }
  }
});

const renderHeadingOpen =
  md.renderer.rules.heading_open || ((tokens, i, options, env, self) => self.renderToken(tokens, i, options));
md.renderer.rules.heading_open = (tokens, i, options, env, self) => {
  tokens[i].attrSet("id", `h-${env.heading++}`);
  return renderHeadingOpen(tokens, i, options, env, self);
};
md.renderer.rules.heading_close = (tokens, i) =>
  `<button type="button" class="target-btn" data-comment-section aria-label="Comment on this section">Comment</button></${tokens[i].tag}>\n`;

md.renderer.rules.fence = (tokens, i) => {
  const token = tokens[i];
  const lang = (token.info || "").trim().split(/\s+/)[0];
  const start = token.attrGet("data-line-start");
  const end = token.attrGet("data-line-end");
  if (lang === "mermaid") {
    return (
      `<figure class="diagram" data-line-start="${start}" data-line-end="${end}">` +
      `<button type="button" class="target-btn" data-comment-diagram aria-label="Comment on this diagram">Comment</button>` +
      `<div class="diagram-body" data-source="${esc(token.content)}"></div></figure>\n`
    );
  }
  return `<pre data-line-start="${start}" data-line-end="${end}"><code>${esc(token.content)}</code></pre>\n`;
};

/** The document's text: in the payload, or in the file it names, of this
 *  round or of the previous one. */
async function documentOf(payload, round) {
  if (typeof payload.markdown === "string") return payload.markdown;
  const name = Pinrail.attachmentName(payload.file);
  if (!name) return "";
  return new TextDecoder().decode(await plugin.attachment(name, round ? { round } : undefined));
}

/** The document of the round this one revises, when there is one. */
async function previousDocument() {
  const payload = plugin.previous && plugin.previous.payload;
  if (!payload) return null;
  try {
    return String(await documentOf(payload, "previous")).replace(/\r\n?/g, "\n");
  } catch {
    return null;
  }
}

async function load(review) {
  const payload = review.payload || {};
  let text;
  try {
    text = await documentOf(payload);
  } catch (error) {
    text = `> The document could not be read: ${String(error && error.message ? error.message : error)}`;
  }
  source = String(text).replace(/\r\n?/g, "\n");
  lines = source.split("\n");
  $("path").textContent = payload.path || review.title || "Markdown";
  const context = $("context");
  if (payload.context) {
    context.hidden = false;
    context.innerHTML = md.render(payload.context, { heading: 10000 });
  }
  headings = outlineOf(source);
  previousSource = await previousDocument();
  document.querySelector('.modes [data-mode="changes"]').hidden = previousSource === null;
  $("shell").dataset.readonly = String(plugin.readonly);
  renderDocument();
  renderChanges();
  renderOutline();
  renderSide();
  setMode(mode, false);
}

/** The headings of the document, from markdown-it's own reading of it. */
function outlineOf(text) {
  const tokens = md.parse(text, { heading: 0 });
  const found = [];
  tokens.forEach((t, i) => {
    if (t.type === "heading_open" && t.map) {
      // the heading's text as it reads, without its markup
      const inline = tokens[i + 1];
      const text =
        (inline.children || []).map((c) => (c.type === "softbreak" ? " " : c.content)).join("") || inline.content;
      found.push({ level: Number(t.tag.slice(1)), text, line: t.map[0] + 1, index: found.length });
    }
  });
  found.forEach((h, i) => {
    const next = found.slice(i + 1).find((n) => n.level <= h.level);
    h.end = next ? next.line - 1 : lines.length;
  });
  return found;
}

/** The headings a line sits under, outermost first. */
function sectionOf(line) {
  const path = [];
  for (const h of headings) {
    if (h.line > line) break;
    while (path.length && path[path.length - 1].level >= h.level) path.pop();
    path.push(h);
  }
  return cut(path.map((h) => h.text).join(" › "), SECTION_MAX);
}

// the decision schema's limits on a comment's section, quote and body
const SECTION_MAX = 1000;
const QUOTE_MAX = 2000;
const BODY_MAX = 4000;

/** `text`, cut to `max` characters with an ellipsis when it is longer. */
function cut(text, max) {
  return text.length > max ? text.slice(0, max - 1) + "…" : text;
}

function renderDocument() {
  $("rendered").innerHTML = md.render(source, { heading: 0 });
  $("raw").innerHTML = lines
    .map(
      (text, i) =>
        `<div class="raw-line" data-line="${i + 1}"><span class="ln">${i + 1}</span><span class="src">${esc(text) || " "}</span></div>`,
    )
    .join("");
  renderDiagrams();
  renderMarks();
}

async function renderDiagrams() {
  const generation = ++renderGeneration;
  const bodies = [...document.querySelectorAll(".diagram-body")];
  if (!bodies.length) return;
  // Mermaid is large, and loaded only for a document that has a diagram
  const mermaid = await loadMermaid();
  if (generation !== renderGeneration) return;
  const dark = document.documentElement.dataset.theme === "dark";
  mermaid.initialize({ startOnLoad: false, securityLevel: "strict", theme: dark ? "dark" : "default" });
  for (const [i, body] of bodies.entries()) {
    const text = body.dataset.source;
    try {
      const { svg } = await mermaid.render(`mermaid-${generation}-${i}`, text);
      if (generation !== renderGeneration) return;
      body.innerHTML = svg;
    } catch (error) {
      if (generation !== renderGeneration) return;
      body.innerHTML = `<div class="diagram-error">The diagram does not render: ${esc(String(error && error.message ? error.message : error))}\n\n${esc(text)}</div>`;
    }
  }
}

// ------------------------------------------------------------------ the outline

function renderOutline() {
  // folded away, or nothing to outline
  const toggle = $("outline-toggle");
  toggle.hidden = !headings.length;
  const open = narrowOutline.matches ? drawer === "outline" : outlineOpen;
  toggle.innerHTML = Pinrail.icon(open ? "panel-left-close" : "panel-left-open", {
    size: 15,
    label: open ? "Hide the outline" : "Show the outline",
  });
  toggle.setAttribute("aria-expanded", String(open));
  const shown = headings.length > 0 && outlineOpen;
  $("work").classList.toggle("no-outline", !shown);
  // in a narrow frame the outline is a drawer, whatever the wide layout's setting
  $("outline").hidden = narrowOutline.matches ? !headings.length : !shown;
  $("outline").classList.toggle("drawer", drawer === "outline");
  if (!headings.length) return;
  const top = Math.min(...headings.map((h) => h.level));
  $("outline-items").innerHTML = headings
    .map((h) => {
      const count = comments.filter((c) => c.target.start_line >= h.line && c.target.start_line <= h.end).length;
      return (
        `<a data-heading="${h.index}" style="padding-left:${8 + (h.level - top) * 12}px" title="${esc(h.text)}">` +
        `<span class="label">${esc(h.text)}</span>${count ? `<span class="count">${count}</span>` : ""}</a>`
      );
    })
    .join("");
}

$("outline-items").addEventListener("click", (event) => {
  const a = event.target.closest("[data-heading]");
  if (!a) return;
  if (drawer) openDrawer(null);
  reveal(headings[Number(a.dataset.heading)].line, headings[Number(a.dataset.heading)].line);
});

/** Scrolls the panes on show to a line, and flashes what holds it. */
function reveal(start, end) {
  if (mode === "changes") {
    let row = $("changes").querySelector(`[data-line="${start}"]`);
    if (!row) {
      const k = folds.findIndex((rows) => rows && rows.some((r) => r.line === start));
      const button = k >= 0 && $("changes").querySelector(`[data-fold="${k}"]`);
      if (button) openFold(button);
      row = $("changes").querySelector(`[data-line="${start}"]`);
    }
    if (row) {
      row.scrollIntoView({ block: "start", behavior: "smooth" });
      flash(row);
    }
    return;
  }
  if (mode !== "raw") {
    const block = blockAt(start);
    if (block) {
      block.scrollIntoView({ block: "start", behavior: "smooth" });
      flash(block);
    }
  }
  if (mode !== "rendered") {
    const row = $("raw").querySelector(`[data-line="${start}"]`);
    if (row) {
      row.scrollIntoView({ block: "start", behavior: "smooth" });
      for (let n = start; n <= end; n++) flash($("raw").querySelector(`[data-line="${n}"]`));
    }
  }
}

function flash(el) {
  if (!el) return;
  el.classList.remove("flash");
  void el.offsetWidth;
  el.classList.add("flash");
}

/** The innermost rendered block whose lines hold `line`. */
function blockAt(line) {
  let best = null;
  for (const el of $("rendered").querySelectorAll("[data-line-start]")) {
    const s = Number(el.dataset.lineStart);
    const e = Number(el.dataset.lineEnd);
    if (s <= line && line <= e && (!best || e - s <= Number(best.dataset.lineEnd) - Number(best.dataset.lineStart)))
      best = el;
  }
  return best;
}

// ------------------------------------------------------------------ changes

/* The Changes mode: the document's source against the previous round's, a
   line at a time. A removed line and the added line that takes its place
   are compared word by word, and the words that changed are marked when the
   two lines are mostly the same; a line rewritten or rewrapped is shown
   whole, as marks on nearly every word would only add noise. Three
   lines of context stay around each change; a longer stretch of unchanged
   lines is folded, and its rows are drawn when it is opened. */
const CONTEXT = 3;
let folds = [];
/** For each of the previous round's comments, by id, whether its lines changed. */
let previousStatus = new Map();

/** The previous round's comments, from its decision. */
const previousComments = () => (plugin.previous && plugin.previous.decision?.data?.comments) || [];

/* Where each of the previous round's comments goes in the diff: after the
   change among its lines, or under its last line when none changed. Its
   lines are counted in the previous version, which the diff's old line
   numbers are. */
function placeComments(rows) {
  const placed = [];
  for (const c of previousComments()) {
    const t = c.target || {};
    const hits = rows
      .map((r, i) => (r.old && r.old >= t.start_line && r.old <= t.end_line ? i : -1))
      .filter((i) => i >= 0);
    if (!hits.length) continue;
    const first = hits[0];
    const last = hits[hits.length - 1];
    let anchor = last;
    // after the whole change its last line is in: the rest of the removed
    // lines and the added ones
    while (anchor + 1 < rows.length && rows[anchor].kind !== "same" && rows[anchor + 1].kind !== "same") anchor++;
    placed.push({ comment: c, anchor, changed: rows.slice(first, anchor + 1).some((r) => r.kind !== "same") });
  }
  return placed;
}

const statusHtml = (changed) =>
  `<span class="pinrail-tone pinrail-tone-${changed ? "success" : "warning"}">${changed ? "Changed" : "Unchanged"}</span>`;

const previousCardHtml = ({ comment, changed }) =>
  `<div class="diff-comment${kindOf(comment) === "question" ? " is-question" : ""}" data-prev-comment="${comment.id}">` +
  `<div class="diff-comment-head"><span>Your comment in the previous round · ${KINDS[kindOf(comment)]}</span>${statusHtml(changed)}</div>` +
  `<p>${esc(comment.body)}</p></div>`;

const splitLines = (value) => (value.endsWith("\n") ? value.slice(0, -1) : value).split("\n");

/** Whether two lines are mostly the same: at least half of the longer one is
 *  in both. */
function similar(words, before, after) {
  const kept = words.filter((w) => !w.added && !w.removed).reduce((n, w) => n + w.value.trim().length, 0);
  return kept >= Math.max(before.trim().length, after.trim().length) / 2;
}

/** The rows of the diff: `same`, `del` or `add`, with their line numbers and text as HTML. */
function diffRows(before, after) {
  const rows = [];
  let o = 1;
  let n = 1;
  const parts = diffLines(before, after);
  for (let i = 0; i < parts.length; i++) {
    const part = parts[i];
    if (!part.added && !part.removed) {
      for (const text of splitLines(part.value)) rows.push({ kind: "same", old: o++, line: n++, html: esc(text) });
      continue;
    }
    if (part.removed && parts[i + 1] && parts[i + 1].added) {
      const gone = splitLines(part.value);
      const come = splitLines(parts[i + 1].value);
      const dels = [];
      const adds = [];
      gone.forEach((text, k) => {
        const words = k < come.length ? diffWordsWithSpace(text, come[k]) : null;
        if (words && similar(words, text, come[k])) {
          const side = (keep, mark) =>
            words
              .filter((w) => !w[keep])
              .map((w) => (w[mark] ? `<mark>${esc(w.value)}</mark>` : esc(w.value)))
              .join("");
          dels.push({ kind: "del", old: o++, line: null, html: side("added", "removed") });
          adds.push({ kind: "add", old: null, line: n++, html: side("removed", "added") });
        } else {
          dels.push({ kind: "del", old: o++, line: null, html: esc(text) });
          if (k < come.length) adds.push({ kind: "add", old: null, line: n++, html: esc(come[k]) });
        }
      });
      for (const text of come.slice(gone.length)) adds.push({ kind: "add", old: null, line: n++, html: esc(text) });
      rows.push(...dels, ...adds);
      i++;
      continue;
    }
    for (const text of splitLines(part.value))
      rows.push(
        part.added
          ? { kind: "add", old: null, line: n++, html: esc(text) }
          : { kind: "del", old: o++, line: null, html: esc(text) },
      );
  }
  return rows;
}

const rowHtml = (r) =>
  `<div class="diff-line ${r.kind}"${r.line ? ` data-line="${r.line}"` : ""}>` +
  `<span class="ln">${r.old ?? ""}</span><span class="ln">${r.line ?? ""}</span>` +
  `<span class="sign">${r.kind === "add" ? "+" : r.kind === "del" ? "−" : ""}</span>` +
  `<span class="src">${r.html || " "}</span></div>`;

function renderChanges() {
  const box = $("changes");
  folds = [];
  if (previousSource === null) {
    box.innerHTML = "";
    return;
  }
  const rows = diffRows(previousSource, source);
  const placed = placeComments(rows);
  previousStatus = new Map(placed.map((p) => [p.comment.id, p.changed]));
  const changed = rows.map((r, i) => (r.kind === "same" ? -1 : i)).filter((i) => i >= 0);
  if (!changed.length) {
    box.innerHTML = `<div class="pinrail-empty">No changes since the previous round.</div>`;
    return;
  }
  // a row stays on show within CONTEXT rows of a change, and so do the lines
  // a previous comment sits under
  const shown = rows.map(
    (_, i) => changed.some((c) => Math.abs(c - i) <= CONTEXT) || placed.some((p) => i <= p.anchor && i >= p.anchor - 1),
  );
  const cardsAfter = (k) =>
    placed
      .filter((p) => p.anchor === k)
      .map(previousCardHtml)
      .join("");
  let html = "";
  let i = 0;
  // the section a group of changes is in, from the document's headings
  const section = (k) => {
    const at = rows.slice(k).find((r) => r.line) || rows[k];
    const name = at && at.line ? sectionOf(at.line) : "";
    return name ? `<div class="diff-section">${esc(name)}</div>` : "";
  };
  while (i < rows.length) {
    if (shown[i]) {
      html += section(i);
      while (i < rows.length && shown[i]) {
        html += rowHtml(rows[i]) + cardsAfter(i);
        i++;
      }
    } else {
      const start = i;
      while (i < rows.length && !shown[i]) i++;
      folds.push(rows.slice(start, i));
      const count = i - start;
      html += `<button type="button" class="diff-fold" data-fold="${folds.length - 1}">${count} unchanged line${count === 1 ? "" : "s"}</button>`;
    }
  }
  box.innerHTML = html;
}

/** Opens a folded stretch, drawing its rows where the fold was. */
function openFold(button) {
  const rows = folds[Number(button.dataset.fold)];
  if (!rows) return;
  button.insertAdjacentHTML("beforebegin", rows.map(rowHtml).join(""));
  button.remove();
}

$("changes").addEventListener("click", (event) => {
  const fold = event.target.closest(".diff-fold");
  if (fold) openFold(fold);
});

// ------------------------------------------------------------------ modes

document.querySelector(".modes").addEventListener("click", (event) => {
  const button = event.target.closest("[data-mode]");
  if (button) setMode(button.dataset.mode, true);
});

function setMode(next, save) {
  mode = next === "raw" || (next === "changes" && previousSource !== null) ? next : "rendered";
  for (const b of document.querySelectorAll(".modes [data-mode]"))
    b.setAttribute("aria-pressed", String(b.dataset.mode === mode));
  $("rendered-pane").hidden = mode !== "rendered";
  $("raw-pane").hidden = mode !== "raw";
  $("changes-pane").hidden = mode !== "changes";
  hidePick();
  // an open dialog follows to the same place in the other pane
  if (!$("dialog").hidden) {
    const text = $("compose-body").value;
    openDialog(anchorFor(dialogTarget()));
    $("compose-body").value = text;
  }
  if (save) saveDraft();
}

// ------------------------------------------------------------------ targets

$("rendered").addEventListener("click", (event) => {
  if (plugin.readonly) return;
  const section = event.target.closest("[data-comment-section]");
  if (section) {
    const heading = headings[Number(section.parentElement.id.slice(2))];
    startComment(
      { kind: "section", start_line: heading.line, end_line: heading.end, section: sectionOf(heading.line) },
      section.parentElement,
    );
    return;
  }
  const diagram = event.target.closest("[data-comment-diagram]");
  if (diagram) {
    const figure = diagram.closest(".diagram");
    const start = Number(figure.dataset.lineStart);
    const end = Number(figure.dataset.lineEnd);
    const first = (figure.querySelector(".diagram-body").dataset.source || "").split("\n")[0].trim();
    startComment(
      { kind: "diagram", start_line: start, end_line: end, section: sectionOf(start), quote: cut(first, QUOTE_MAX) },
      figure,
    );
  }
});

// a passage selected in either pane: a button to comment on it
let picked = null;
/** where on screen the selection ends, for the dialog */
let pickedAt = null;
const inDialog = (event) => event.target instanceof Element && event.target.closest("#dialog");
document.addEventListener("mouseup", (event) => {
  if (!inDialog(event)) setTimeout(offerPick, 0);
});
document.addEventListener("keyup", (event) => {
  if (!inDialog(event) && (event.shiftKey || event.key.startsWith("Arrow"))) offerPick();
});

function offerPick() {
  if (plugin.readonly) return;
  const selection = window.getSelection();
  const text = selection ? selection.toString().trim() : "";
  if (!text || selection.rangeCount === 0) return hidePick();
  const range = selection.getRangeAt(0);
  const lineRange = linesOf(range);
  if (!lineRange) return hidePick();
  picked = {
    kind: "text",
    start_line: lineRange[0],
    end_line: lineRange[1],
    section: sectionOf(lineRange[0]),
    quote: text.slice(0, QUOTE_MAX),
  };
  // just under the end of the selection's last line
  const rects = [...range.getClientRects()].filter((r) => r.width > 0 || r.height > 0);
  const end = rects.length ? rects[rects.length - 1] : range.getBoundingClientRect();
  pickedAt = { left: rects.length ? rects[0].left : end.left, bottom: end.bottom };
  const docs = $("docs").getBoundingClientRect();
  const pick = $("pick");
  pick.hidden = false;
  const width = pick.offsetWidth || 80;
  pick.style.left = `${Math.max(8, Math.min(end.right - docs.left - width / 2, docs.width - width - 8))}px`;
  pick.style.top = `${Math.max(4, end.bottom - docs.top + 6)}px`;
}

function hidePick() {
  picked = null;
  $("pick").hidden = true;
}

// the button stays where it was put, so a pane that scrolls takes it away
for (const pane of ["rendered-pane", "raw-pane", "changes-pane"])
  $(pane).addEventListener("scroll", hidePick, { passive: true });

$("pick").addEventListener("mousedown", (event) => event.preventDefault());
$("pick").addEventListener("click", () => {
  if (!picked) return;
  const target = picked;
  const at = pickedAt;
  hidePick();
  window.getSelection().removeAllRanges();
  startComment(target, at);
});

/** The source lines a selection spans, from the blocks or rows it touches. */
function linesOf(range) {
  const at = (node) => {
    const el = node.nodeType === 1 ? node : node.parentElement;
    if (!el) return null;
    const row = el.closest(".raw-line");
    if (row && $("raw").contains(row)) return [Number(row.dataset.line), Number(row.dataset.line)];
    const block = el.closest("[data-line-start]");
    if (block && $("rendered").contains(block)) return [Number(block.dataset.lineStart), Number(block.dataset.lineEnd)];
    return null;
  };
  const a = at(range.startContainer);
  const b = at(range.endContainer);
  if (!a || !b) return null;
  // within one block, the lines the text sits on when it can be found there
  let start = Math.min(a[0], b[0]);
  let end = Math.max(a[1], b[1]);
  const first = range.toString().trim().split("\n")[0].trim().slice(0, 60);
  if (first && a[0] === b[0] && a[1] === b[1] && end > start) {
    const hit = lines.slice(start - 1, end).findIndex((l) => l.includes(first));
    if (hit >= 0) {
      start += hit;
      const lineCount = range.toString().trim().split("\n").length;
      end = Math.min(end, start + lineCount - 1);
    }
  }
  return [start, end];
}

// ------------------------------------------------------------------ comments

function startComment(target, anchor) {
  composing = target;
  editing = null;
  openDialog(anchor);
}

function editComment(id) {
  const comment = comments.find((c) => c.id === id);
  if (!comment) return;
  composing = null;
  editing = id;
  const anchor = anchorFor(comment.target);
  if (anchor) anchor.scrollIntoView({ block: "center" });
  openDialog(anchor);
}

// ------------------------------------------------------------------ the comment dialog

/** What the open dialog is about: a new comment's target, or the edited one's. */
function dialogTarget() {
  if (editing !== null) return comments.find((c) => c.id === editing)?.target || null;
  return composing;
}

/** The element on show that a target's dialog sits under. */
function anchorFor(target) {
  const line = target.kind === "section" ? target.start_line : target.end_line;
  if (mode === "raw") return $("raw").querySelector(`[data-line="${line}"]`);
  return blockAt(line);
}

/** Opens the dialog in the pane on show, under `anchor`: an element, or a
 *  place on screen with `left` and `bottom`. It scrolls with the text. */
function openDialog(anchor) {
  const target = dialogTarget();
  if (!target) return;
  // comments are written in Raw from the Changes mode, against the document as it is
  if (mode === "changes") {
    setMode("raw", true);
    anchor = anchorFor(target);
  }
  const pane = $(mode === "raw" ? "raw-pane" : "rendered-pane");
  const dialog = $("dialog");
  if (dialog.parentElement !== pane) pane.appendChild(dialog);
  const edited = editing !== null ? comments.find((c) => c.id === editing) : null;
  const kind = edited ? kindOf(edited) : "change";
  dialog.innerHTML =
    where(target) +
    // a change to make, or a question for the agent to answer
    `<div class="kinds" role="radiogroup" aria-label="Kind of comment">` +
    Object.entries(KINDS)
      .map(
        ([value, label]) =>
          `<label class="${value === kind ? "on" : ""}"><input type="radio" name="compose-kind" value="${value}" ${value === kind ? "checked" : ""}> ${label}</label>`,
      )
      .join("") +
    `</div>` +
    // the SDK's comment field, with the microphone and the buttons at its end
    `<div class="pinrail-compose">` +
    `<textarea class="pinrail-compose-input" id="compose-body" rows="1" maxlength="${BODY_MAX}" data-pinrail-dictate placeholder="${PLACEHOLDERS[kind]}" aria-label="Comment">${edited ? esc(edited.body) : ""}</textarea>` +
    `<span class="pinrail-compose-keys">Enter to ${edited ? "save" : "add"} · Shift+Enter for a new line</span>` +
    `<span class="pinrail-compose-tools"><span data-pinrail-dictate-for="compose-body"></span>` +
    `<button type="button" class="pinrail-compose-icon pinrail-compose-cancel" id="compose-cancel" title="Cancel (Esc)" aria-label="Cancel"></button>` +
    `<button type="button" class="pinrail-compose-icon pinrail-compose-send" id="compose-save" title="${edited ? "Save" : "Comment"} (Enter)" aria-label="${edited ? "Save" : "Comment"}"></button>` +
    `</span></div>`;
  dialog.hidden = false;
  const box = pane.getBoundingClientRect();
  const at =
    anchor instanceof Element
      ? anchor.getBoundingClientRect()
      : anchor || { left: box.left + 36, bottom: box.top + 16 };
  const width = dialog.offsetWidth;
  dialog.style.left = `${Math.max(8, Math.min(at.left - box.left, pane.clientWidth - width - 8))}px`;
  dialog.style.top = `${at.bottom - box.top + pane.scrollTop + 6}px`;
  markTarget(target);
  dialog.scrollIntoView({ block: "nearest" });
  const area = $("compose-body");
  fitCompose(area);
  area.focus();
  area.setSelectionRange(area.value.length, area.value.length);
}

/** Sizes the field as the SDK does, and greys its add button while it is empty. */
function fitCompose(area) {
  Pinrail.fitField(area);
  $("compose-save").disabled = !area.value.trim();
}

function closeDialog() {
  composing = null;
  editing = null;
  $("dialog").hidden = true;
  markTarget(null);
}

function saveDialog() {
  const body = $("compose-body").value.trim();
  if (!body) return;
  const kind = document.querySelector('input[name="compose-kind"]:checked')?.value || "change";
  if (editing !== null) {
    const comment = comments.find((c) => c.id === editing);
    if (comment) Object.assign(comment, { body, kind });
  } else if (composing) {
    const id = comments.reduce((max, c) => Math.max(max, c.id), 0) + 1;
    comments.push({ id, kind, target: composing, body });
    comments.sort((a, b) => a.target.start_line - b.target.start_line || a.id - b.id);
  }
  closeDialog();
  changed();
}

/** Shades what the open dialog is about, in both panes. */
function markTarget(target) {
  const hit = (s, e) => target && target.start_line <= e && s <= target.end_line;
  for (const el of $("rendered").querySelectorAll(":scope > [data-line-start]")) {
    el.classList.toggle("targeted", !!hit(Number(el.dataset.lineStart), Number(el.dataset.lineEnd)));
  }
  for (const row of $("raw").querySelectorAll(".raw-line")) {
    const n = Number(row.dataset.line);
    row.classList.toggle("targeted", !!hit(n, n));
  }
}

// the kind changes what the box asks for
$("dialog").addEventListener("change", (event) => {
  if (event.target.name !== "compose-kind") return;
  for (const label of $("dialog").querySelectorAll(".kinds label")) {
    label.classList.toggle("on", label.contains(event.target));
  }
  $("compose-body").placeholder = PLACEHOLDERS[event.target.value];
  $("compose-body").focus();
});
$("dialog").addEventListener("input", (event) => {
  if (event.target.id === "compose-body") fitCompose(event.target);
});
$("dialog").addEventListener("click", (event) => {
  if (event.target.id === "compose-save") saveDialog();
  else if (event.target.id === "compose-cancel") closeDialog();
});
$("dialog").addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    event.preventDefault();
    closeDialog();
  } else if (event.key === "Enter" && !event.shiftKey && !event.isComposing && event.target.id === "compose-body") {
    event.preventDefault();
    saveDialog();
  }
});

function changed() {
  sideTab = "current";
  saveDraft();
  renderSide();
  renderOutline();
  renderMarks();
}

function saveDraft() {
  plugin.draft({ comments, mode });
  plugin.handOverLabel(statusLabel());
}

function statusLabel() {
  const n = comments.length;
  const questions = comments.filter((c) => kindOf(c) === "question").length;
  const changes = n - questions;
  const count = (k, word) => `${k} ${word}${k === 1 ? "" : "s"}`;
  if (changes)
    return `Request changes · ${[changes && count(changes, "change"), questions && count(questions, "question")].filter(Boolean).join(", ")}`;
  if (questions) return `Ask · ${count(questions, "question")}`;
  return "Approve";
}

/** Marks what has comments, in both panes. */
function renderMarks() {
  const shown =
    plugin.readonly && plugin.review && plugin.review.decision ? plugin.review.decision.data.comments || [] : comments;
  const hit = (s, e) => shown.some((c) => c.target.start_line <= e && s <= c.target.end_line);
  for (const el of $("rendered").querySelectorAll(":scope > [data-line-start]")) {
    el.classList.toggle("commented", hit(Number(el.dataset.lineStart), Number(el.dataset.lineEnd)));
  }
  for (const row of $("raw").querySelectorAll(".raw-line")) {
    const n = Number(row.dataset.line);
    row.classList.toggle("commented", hit(n, n));
  }
  drawBars();
  const n = shown.length;
  $("tally").innerHTML = `<b>${n}</b> comment${n === 1 ? "" : "s"}`;
  $("comments-toggle").innerHTML =
    `${Pinrail.icon("message-square", { size: 13 })}<span><b>${n}</b> comment${n === 1 ? "" : "s"}</span>`;
}

/** A straight bar in the margin beside each commented block, placed from
 *  where the blocks are now; redrawn as the document changes size. */
function drawBars() {
  const article = $("rendered");
  let layer = $("bars");
  if (!layer) {
    layer = document.createElement("div");
    layer.id = "bars";
    layer.className = "bars";
    layer.setAttribute("aria-hidden", "true");
  }
  if (layer.parentElement !== article) article.prepend(layer);
  layer.innerHTML = [...article.querySelectorAll(":scope > .commented")]
    .map((el) => `<span style="top:${el.offsetTop}px;height:${el.offsetHeight}px"></span>`)
    .join("");
}
new ResizeObserver(() => drawBars()).observe($("rendered"));

function where(target, comment, actions = "") {
  const lineText =
    target.start_line === target.end_line
      ? `line ${target.start_line}`
      : `lines ${target.start_line}–${target.end_line}`;
  return (
    `<div class="where">${comment && kindOf(comment) === "question" ? `<span class="question-tag">Question</span>` : ""}<span class="kind">${esc(target.kind)}</span><span class="section" title="${esc(target.section || "")}">${esc(target.section || "")}</span><span class="lines">${lineText}</span>${actions}</div>` +
    (target.quote && target.kind === "text" ? `<div class="quote">${esc(target.quote)}</div>` : "")
  );
}

function renderSide() {
  const readonly = plugin.readonly;
  const decided = plugin.review && plugin.review.decision ? plugin.review.decision.data : null;
  const shown = readonly ? (decided ? decided.comments || [] : []) : comments;
  const previous = plugin.previous && plugin.previous.decision ? plugin.previous.decision.data : null;
  const earlier = (previous && previous.comments) || [];
  let html = "";
  // this round's comments and the previous round's, in two tabs when there are both
  if (earlier.length) {
    const tab = (id, label, count) =>
      `<button type="button" role="tab" data-side-tab="${id}" aria-selected="${sideTab === id}">${label}<span class="count">${count}</span></button>`;
    html +=
      `<div class="side-tabs" role="tablist" aria-label="Comments of">` +
      tab("current", "This round", shown.length) +
      tab("previous", "Previous round", earlier.length) +
      `</div>`;
  }
  if (earlier.length && sideTab === "previous") {
    html +=
      `<div class="previous">` +
      earlier
        .map(
          (c) =>
            `<div class="card${kindOf(c) === "question" ? " is-question" : ""}"${previousStatus.has(c.id) ? ` data-prev="${c.id}"` : ""}>${where(c.target, c, previousStatus.has(c.id) ? statusHtml(previousStatus.get(c.id)) : "")}<p class="body">${esc(c.body)}</p></div>`,
        )
        .join("") +
      `</div>`;
  } else {
    if (!shown.length) {
      html += readonly
        ? `<div class="pinrail-empty">No comments.</div>`
        : `<div class="pinrail-empty">Hover a heading or a diagram and choose Comment, or select a passage in either pane.</div>`;
    }
    for (const c of shown) {
      // edit and delete at the right of the header, and a click on the text edits it too
      const actions = readonly
        ? ""
        : `<span class="pinrail-comment-actions"><button type="button" class="pinrail-comment-edit" data-edit="${c.id}" title="Edit the comment" aria-label="Edit the comment"></button><button type="button" class="pinrail-comment-delete" data-delete="${c.id}" title="Delete the comment" aria-label="Delete the comment"></button></span>`;
      html += `<div class="card pinrail-comment${kindOf(c) === "question" ? " is-question" : ""}" data-comment="${c.id}">${where(c.target, c, actions)}<p class="body${readonly ? "" : " is-editable"}">${esc(c.body)}</p></div>`;
    }
  }
  $("list").innerHTML = html;
  renderVerdict(readonly, decided);
  plugin.handOverLabel(statusLabel());
  syncBar();
}

function renderVerdict(readonly, decided) {
  if (readonly) {
    $("verdict").innerHTML = decided
      ? `<div class="decided"><b>${decided.verdict === "approve" ? "Approved" : "Changes requested"}</b></div>`
      : `<div class="decided pinrail-dim">Closed without a decision (${esc(plugin.review.status)})</div>`;
    return;
  }
  // the verdict follows from the comments: none approves, any request changes
  $("verdict").innerHTML = "";
}

$("list").addEventListener("click", (event) => {
  const t = event.target;
  const tab = t.closest("[data-side-tab]");
  if (tab) {
    sideTab = tab.dataset.sideTab;
    return renderSide();
  }
  // close the drawer so that the document shows what the comment refers to
  if (drawer && (t.dataset.edit || (t.closest("[data-comment]") && !t.closest("button")))) openDrawer(null);
  if (t.dataset.edit) return editComment(Number(t.dataset.edit));
  if (t.dataset.delete) {
    const id = Number(t.dataset.delete);
    if (editing === id) closeDialog();
    comments = comments.filter((c) => c.id !== id);
    return changed();
  }
  // a previous round's comment: to its place in the Changes view
  const prev = t.closest("[data-prev]");
  if (prev) {
    setMode("changes", true);
    const card = $("changes").querySelector(`[data-prev-comment="${prev.dataset.prev}"]`);
    if (card) {
      card.scrollIntoView({ block: "center", behavior: "smooth" });
      flash(card);
    }
    return;
  }
  // a click on a comment's text shows what it is about and edits it
  const body = t.closest(".body.is-editable");
  if (body) {
    const c = comments.find((x) => x.id === Number(body.closest("[data-comment]").dataset.comment));
    if (!c) return;
    reveal(c.target.start_line, c.target.end_line);
    return editComment(c.id);
  }
  // a comment in the list: to what it is about
  const card = t.closest("[data-comment]");
  if (card && !t.closest("button")) {
    const id = Number(card.dataset.comment);
    const shown = plugin.readonly ? plugin.review.decision?.data?.comments || [] : comments;
    const c = shown.find((x) => x.id === id);
    if (c) reveal(c.target.start_line, c.target.end_line);
  }
});

// ------------------------------------------------------------------ hand-over

/* No comments approves the document; any change requests changes; questions
   alone ask the agent to explain. */
function handOver() {
  violations = [];
  if (!$("dialog").hidden && $("compose-body").value.trim()) saveDialog();
  const changes = comments.some((c) => kindOf(c) === "change");
  return {
    verdict: changes ? "request_changes" : comments.length ? "questions" : "approve",
    comments: comments.map((c) => ({ ...c, kind: kindOf(c) })),
  };
}

$("outline-toggle").addEventListener("click", () => {
  if (narrowOutline.matches) return openDrawer(drawer === "outline" ? null : "outline");
  outlineOpen = !outlineOpen;
  plugin.setSetting("outline_open", outlineOpen);
  renderOutline();
});
$("comments-toggle").addEventListener("click", () => openDrawer(drawer === "comments" ? null : "comments"));
$("side-toggle").addEventListener("click", () => {
  commentsOpen = !commentsOpen;
  plugin.setSetting("comments_open", commentsOpen);
  renderCommentsPanel();
});
$("scrim").addEventListener("click", () => openDrawer(null));
window.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && drawer && !event.defaultPrevented) {
    event.preventDefault();
    openDrawer(null);
  }
});
for (const query of [narrowOutline, narrowComments]) {
  query.addEventListener("change", () => {
    if (drawer === "outline" && !narrowOutline.matches) drawer = null;
    if (drawer === "comments" && !narrowComments.matches) drawer = null;
    renderDrawers();
  });
}

/** Opens the outline or the comments as a drawer, or closes it. Focus goes
 *  into the drawer, and back to its button when it closes. */
function openDrawer(which) {
  const was = drawer;
  if (was === which) return;
  drawer = which;
  renderDrawers();
  if (which) $(which === "outline" ? "outline" : "side").focus();
  else $(was === "outline" ? "outline-toggle" : "comments-toggle").focus();
}

function renderDrawers() {
  $("work").classList.toggle("drawer-open", drawer !== null);
  $("side").classList.toggle("drawer", drawer === "comments");
  $("comments-toggle").setAttribute("aria-expanded", String(drawer === "comments"));
  renderOutline();
  renderCommentsPanel();
}

/** The comments beside the document, or folded away. In a narrow frame
 *  they are a drawer, whatever this setting says. */
function renderCommentsPanel() {
  const toggle = $("side-toggle");
  toggle.innerHTML = Pinrail.icon(commentsOpen ? "panel-right-close" : "panel-right-open", {
    size: 15,
    label: commentsOpen ? "Hide the comments" : "Show the comments",
  });
  toggle.setAttribute("aria-expanded", String(commentsOpen));
  $("work").classList.toggle("no-side", !commentsOpen);
}
$("shell").append(confirm.element);
