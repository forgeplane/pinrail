// A fenced `diff` block in an agent's Markdown, shown as a small diff view:
// one file at a time, under a bar that names it, with a row for each hunk
// and each line with its old and new line numbers and a mark for an added
// or a deleted line. The code of each line is highlighted in its file's
// language, when the app knows it.

import { highlight } from "./highlight";

type Row =
  | { kind: "file"; path: string }
  | { kind: "hunk"; text: string }
  | { kind: "add" | "del" | "same"; old: number | null; new: number | null; text: string }
  | { kind: "note"; text: string };

const HUNK = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/;
// the lines between `diff --git` and the first hunk that say what changed
// about the file rather than in it
const FILE_LINE =
  /^(index |new file mode|deleted file mode|old mode|new mode|similarity index|rename from|rename to|copy from|copy to|Binary files)/;

/** A path from a `---` or `+++` line, without its a/ or b/ and any time
 *  stamp after a tab; null for /dev/null. */
function pathOf(line: string): string | null {
  const path = line.slice(4).split("\t")[0].trim();
  if (path === "/dev/null") return null;
  return path.replace(/^[ab]\//, "");
}

/** The rows of a unified diff, as `git diff` and `diff -u` write it. A diff
 *  without hunk headers has lines without numbers. */
export function diffRows(text: string): Row[] {
  const lines = text.replace(/\n$/, "").split("\n");
  const rows: Row[] = [];
  let old: number | null = null;
  let now: number | null = null;
  // between a file's first line and its first hunk
  let header = false;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (line.startsWith("diff --git ")) {
      const path = line.match(/ b\/(.+)$/)?.[1] ?? line.slice(11);
      rows.push({ kind: "file", path });
      header = true;
      continue;
    }
    if (header && FILE_LINE.test(line)) continue;
    // a file's names: the new one, or the old one for a deleted file
    if (line.startsWith("--- ") && lines[i + 1]?.startsWith("+++ ")) {
      const path = pathOf(lines[i + 1]) ?? pathOf(line) ?? "";
      const last = rows[rows.length - 1];
      if (header && last?.kind === "file") last.path = path;
      else rows.push({ kind: "file", path });
      header = true;
      i++;
      continue;
    }
    const hunk = line.match(HUNK);
    if (hunk) {
      old = Number(hunk[1]);
      now = Number(hunk[2]);
      rows.push({ kind: "hunk", text: line });
      header = false;
      continue;
    }
    if (line.startsWith("\\")) {
      rows.push({ kind: "note", text: line.slice(1).trim() });
      continue;
    }
    const sign = line[0];
    if (sign === "+") {
      rows.push({ kind: "add", old: null, new: now, text: line.slice(1) });
      if (now !== null) now++;
    } else if (sign === "-") {
      rows.push({ kind: "del", old, new: null, text: line.slice(1) });
      if (old !== null) old++;
    } else {
      // a context line, or one whose leading space was trimmed
      rows.push({ kind: "same", old, new: now, text: sign === " " ? line.slice(1) : line });
      if (old !== null) old++;
      if (now !== null) now++;
    }
  }
  return rows;
}

const LANGUAGE: Record<string, string> = {
  sh: "bash",
  bash: "bash",
  css: "css",
  ex: "elixir",
  exs: "elixir",
  go: "go",
  js: "javascript",
  jsx: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  json: "json",
  py: "python",
  rs: "rust",
  sql: "sql",
  ts: "typescript",
  tsx: "typescript",
  html: "xml",
  xml: "xml",
  svg: "xml",
  yml: "yaml",
  yaml: "yaml",
};

/** The highlight.js language of a file, by its extension. */
const languageOf = (path: string) => LANGUAGE[path.split(".").pop()?.toLowerCase() ?? ""] ?? "";

const escape = (text: string) =>
  text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

const SIGN = { add: "+", del: "-", same: " " } as const;
const LABEL = { add: "added", del: "deleted", same: "" } as const;

// lucide's chevron-left and chevron-right, drawn in the button's colour
const chevron = (path: string) =>
  `<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="${path}"/></svg>`;

type File = { path: string | null; rows: Row[] };

/** The files of a diff, each with its rows; lines before any file header
 *  are a file without a name. */
function diffFiles(text: string): { files: File[]; numbered: boolean } {
  const rows = diffRows(text);
  const files: File[] = [];
  for (const row of rows) {
    if (row.kind === "file") files.push({ path: row.path, rows: [] });
    else {
      if (!files.length) files.push({ path: null, rows: [] });
      files[files.length - 1].rows.push(row);
    }
  }
  return { files, numbered: rows.some((row) => row.kind === "hunk") };
}

/** One file of a diff as a table with a fixed layout. */
function fileTable(file: File, numbered: boolean): string {
  const span = numbered ? 3 : 1;
  const language = file.path ? languageOf(file.path) : "";
  const cols = numbered ? '<colgroup><col class="md-diff-ncol"><col class="md-diff-ncol"><col></colgroup>' : "";
  const html = file.rows.map((row) => {
    if (row.kind === "hunk") return `<tr class="md-diff-hunk"><td colspan="${span}">${escape(row.text)}</td></tr>`;
    if (row.kind === "note" || row.kind === "file")
      return `<tr class="md-diff-note"><td colspan="${span}">${escape(row.kind === "note" ? row.text : row.path)}</td></tr>`;
    const code = highlight(row.text, language) ?? escape(row.text);
    const label = LABEL[row.kind] ? ` aria-label="${LABEL[row.kind]}"` : "";
    const numbers = numbered
      ? `<td class="md-diff-n">${row.old ?? ""}</td><td class="md-diff-n">${row.new ?? ""}</td>`
      : "";
    return (
      `<tr class="md-diff-line md-diff-${row.kind}">${numbers}` +
      `<td class="md-diff-code"><span class="md-diff-sign"${label}>${SIGN[row.kind]}</span>${code}</td></tr>`
    );
  });
  return `<table>${cols}<tbody>${html.join("")}</tbody></table>`;
}

/** A unified diff as a diff view: one file at a time, with a bar that names
 *  it and, for several files, buttons that step to the others. Only the
 *  file shown is in the page: the view keeps the diff's text, and draws
 *  another file when it is stepped to, so a session with many diffs opens
 *  quickly. */
export function renderDiff(text: string): string {
  const { files, numbered } = diffFiles(text);
  const first = files[0] ?? { path: null, rows: [] };
  const several = files.length > 1;
  const bar =
    several || first.path
      ? `<div class="md-diff-bar"><span class="md-diff-path truncate">${escape(first.path ?? "")}</span>` +
        (several
          ? `<span class="md-diff-count">1 of ${files.length}</span>` +
            `<button type="button" class="md-diff-step" data-diff-step="-1" aria-label="Previous file" title="Previous file" disabled>${chevron("m15 18-6-6 6-6")}</button>` +
            `<button type="button" class="md-diff-step" data-diff-step="1" aria-label="Next file" title="Next file">${chevron("m9 18 6-6-6-6")}</button>`
          : "") +
        "</div>"
      : "";
  const source = several ? ` data-source="${escape(text)}"` : "";
  return `<div class="md-diff" data-file="0" data-files="${files.length}"${source}>${bar}${fileTable(first, numbered)}</div>\n`;
}

/** Steps a diff view to the file before or after the one it shows, for its
 *  previous and next buttons. */
export function stepDiff(button: HTMLElement) {
  const view = button.closest<HTMLElement>(".md-diff");
  const source = view?.dataset.source;
  if (!view || source === undefined) return;
  const { files, numbered } = diffFiles(source);
  const at = Math.min(files.length - 1, Math.max(0, Number(view.dataset.file) + Number(button.dataset.diffStep)));
  view.dataset.file = String(at);
  view.querySelector("table")?.insertAdjacentHTML("afterend", fileTable(files[at], numbered));
  view.querySelector("table")?.remove();
  const path = view.querySelector(".md-diff-path");
  if (path) path.textContent = files[at].path ?? "";
  const shown = view.querySelector(".md-diff-count");
  if (shown) shown.textContent = `${at + 1} of ${files.length}`;
  for (const step of view.querySelectorAll<HTMLButtonElement>("[data-diff-step]")) {
    step.disabled = Number(step.dataset.diffStep) < 0 ? at === 0 : at === files.length - 1;
  }
}
