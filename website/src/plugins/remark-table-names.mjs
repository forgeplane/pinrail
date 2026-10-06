// Inline code in a table cell with no space in it, such as `visual-diff` or
// `--wait`, is a name: it gets the class pr-name, and the stylesheet keeps it
// on one line instead of breaking it at a hyphen. Code with spaces, such as
// a command with its options, still wraps at the spaces.
import { visit } from "unist-util-visit";

export default function remarkTableNames() {
  return (tree) => {
    visit(tree, "tableCell", (cell) => {
      visit(cell, "inlineCode", (code) => {
        if (/\s/.test(code.value)) return;
        code.data = { ...code.data, hProperties: { ...code.data?.hProperties, className: ["pr-name"] } };
      });
    });
  };
}
