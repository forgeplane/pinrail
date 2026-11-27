// Extra arguments for an agent, written as a shell would split them:
// words separated by spaces, with single or double quotes around a word
// that holds spaces, and a backslash before a character to keep it. On
// Windows, as a Windows program splits its command line, where a
// backslash is a folder's separator, as in `--add-dir C:\code\app`.

/** Whether the window runs on Windows. */
const onWindows = /Win/i.test(typeof navigator === "undefined" ? "" : navigator.platform);

/** The words of `text`; an unclosed quote runs to the end. */
export function splitArgs(text: string, windows = onWindows): string[] {
  if (windows) return splitWindows(text);
  const words: string[] = [];
  let word = "";
  let inWord = false;
  let quote: '"' | "'" | null = null;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quote) {
      if (c === quote) quote = null;
      else if (c === "\\" && quote === '"' && i + 1 < text.length) word += text[++i];
      else word += c;
    } else if (c === '"' || c === "'") {
      quote = c;
      inWord = true;
    } else if (c === "\\" && i + 1 < text.length) {
      word += text[++i];
      inWord = true;
    } else if (/\s/.test(c)) {
      if (inWord) words.push(word);
      word = "";
      inWord = false;
    } else {
      word += c;
      inWord = true;
    }
  }
  if (inWord) words.push(word);
  return words;
}

/**
 * The words of `text` by the rules of Windows programs: spaces part them
 * outside double quotes; backslashes are themselves, but before a `"`,
 * where each pair is one backslash and an odd one makes the `"` a
 * character; and `""` inside quotes is one `"`.
 */
function splitWindows(text: string): string[] {
  const words: string[] = [];
  let word = "";
  let inWord = false;
  let quoted = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === "\\") {
      let slashes = 0;
      while (text[i] === "\\") {
        slashes++;
        i++;
      }
      if (text[i] === '"') {
        word += "\\".repeat(slashes >> 1);
        if (slashes % 2 === 1) word += '"';
        else i--;
      } else {
        word += "\\".repeat(slashes);
        i--;
      }
      inWord = true;
    } else if (c === '"' && quoted && text[i + 1] === '"') {
      word += '"';
      i++;
    } else if (c === '"') {
      quoted = !quoted;
      inWord = true;
    } else if (!quoted && /\s/.test(c)) {
      if (inWord) words.push(word);
      word = "";
      inWord = false;
    } else {
      word += c;
      inWord = true;
    }
  }
  if (inWord) words.push(word);
  return words;
}
