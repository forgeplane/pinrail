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
/** every heading: level, text, the line it starts on, its section's last line */
let headings = [];
let comments = [];
let mode = "rendered";
/** what a new comment is about, while it is being written */
let composing = null;
/** the comment being edited */
let editing = null;
let renderGeneration = 0;
/** the outline beside the document: shown unless the person folded it away */
let outlineOpen = true;
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
}

const plugin = Pinrail.connect({
  onInit({ review, draft, settings }) {
    applySettings(settings);
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

/** The document's text: in the payload, or in the file it names. */
async function documentOf(payload) {
  if (typeof payload.markdown === "string") return payload.markdown;
  const name = Pinrail.attachmentName(payload.file);
  if (!name) return "";
  return new TextDecoder().decode(await plugin.attachment(name));
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
  $("shell").dataset.readonly = String(plugin.readonly);
  renderDocument();
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
  return path.map((h) => h.text).join(" › ");
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
  toggle.innerHTML = Pinrail.icon(outlineOpen ? "panel-left-close" : "panel-left-open", {
    size: 15,
    label: outlineOpen ? "Hide the outline" : "Show the outline",
  });
  const shown = headings.length > 0 && outlineOpen;
  $("work").classList.toggle("no-outline", !shown);
  $("outline").hidden = !shown;
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
  if (a) reveal(headings[Number(a.dataset.heading)].line, headings[Number(a.dataset.heading)].line);
});

/** Scrolls the panes on show to a line, and flashes what holds it. */
function reveal(start, end) {
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

// ------------------------------------------------------------------ modes

document.querySelector(".modes").addEventListener("click", (event) => {
  const button = event.target.closest("[data-mode]");
  if (button) setMode(button.dataset.mode, true);
});

function setMode(next, save) {
  mode = next === "raw" ? "raw" : "rendered";
  for (const b of document.querySelectorAll(".modes [data-mode]"))
    b.setAttribute("aria-pressed", String(b.dataset.mode === mode));
  $("rendered-pane").hidden = mode === "raw";
  $("raw-pane").hidden = mode === "rendered";
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
      { kind: "diagram", start_line: start, end_line: end, section: sectionOf(start), quote: first },
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
    quote: text.slice(0, 2000),
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
for (const pane of ["rendered-pane", "raw-pane"]) $(pane).addEventListener("scroll", hidePick, { passive: true });

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
    `<textarea id="compose-body" placeholder="${PLACEHOLDERS[kind]}" aria-label="Comment">${edited ? esc(edited.body) : ""}</textarea>` +
    `<div class="foot"><span class="hint"><kbd>Enter</kbd> to ${edited ? "save" : "add"}, <kbd>Esc</kbd> to cancel</span>` +
    `<button type="button" class="pinrail-btn pinrail-btn-ghost" id="compose-cancel">Cancel</button>` +
    `<button type="button" class="pinrail-btn pinrail-btn-primary" id="compose-save">${edited ? "Save" : "Comment"}</button></div>`;
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
  area.focus();
  area.setSelectionRange(area.value.length, area.value.length);
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

function where(target, comment) {
  const lineText =
    target.start_line === target.end_line
      ? `line ${target.start_line}`
      : `lines ${target.start_line}–${target.end_line}`;
  return (
    `<div class="where">${comment && kindOf(comment) === "question" ? `<span class="question-tag">Question</span>` : ""}<span class="kind">${esc(target.kind)}</span><span class="section" title="${esc(target.section || "")}">${esc(target.section || "")}</span><span class="lines">${lineText}</span></div>` +
    (target.quote && target.kind === "text" ? `<div class="quote">${esc(target.quote)}</div>` : "")
  );
}

function renderSide() {
  const readonly = plugin.readonly;
  const decided = plugin.review && plugin.review.decision ? plugin.review.decision.data : null;
  const shown = readonly ? (decided ? decided.comments || [] : []) : comments;
  let html = "";
  if (!shown.length) {
    html += readonly
      ? `<div class="pinrail-empty">No comments.</div>`
      : `<div class="pinrail-empty">Hover a heading or a diagram and choose Comment, or select a passage in either pane.</div>`;
  }
  for (const c of shown) {
    html += `<div class="card${kindOf(c) === "question" ? " is-question" : ""}" data-comment="${c.id}">${where(c.target, c)}<p class="body">${esc(c.body)}</p>`;
    if (!readonly)
      html += `<div class="actions"><button type="button" data-edit="${c.id}">Edit</button><button type="button" data-delete="${c.id}">Delete</button></div>`;
    html += `</div>`;
  }
  const previous = plugin.previous && plugin.previous.decision ? plugin.previous.decision.data : null;
  if (previous && previous.comments && previous.comments.length) {
    html +=
      `<details class="previous"><summary>The previous round: ${previous.comments.length} comment${previous.comments.length === 1 ? "" : "s"}</summary>` +
      previous.comments
        .map(
          (c) =>
            `<div class="card${kindOf(c) === "question" ? " is-question" : ""}">${where(c.target, c)}<p class="body">${esc(c.body)}</p></div>`,
        )
        .join("") +
      `</details>`;
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
  if (t.dataset.edit) return editComment(Number(t.dataset.edit));
  if (t.dataset.delete) {
    const id = Number(t.dataset.delete);
    if (editing === id) closeDialog();
    comments = comments.filter((c) => c.id !== id);
    return changed();
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
  outlineOpen = !outlineOpen;
  plugin.setSetting("outline_open", outlineOpen);
  renderOutline();
});
$("shell").append(confirm.element);
