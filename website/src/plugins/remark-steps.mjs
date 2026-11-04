// Steps to follow in order, in the docs and the blog. A numbered list in a
// `:::steps` block becomes a timeline: each step's number in a badge, and a
// line that joins the badges. src/styles/steps.css draws it.
//
//   :::steps
//   1. Open *Settings › Labs*.
//   2. Turn on *Sessions and questions*.
//   :::
import { visit } from "unist-util-visit";

export default function remarkSteps() {
  return (tree, file) => {
    visit(tree, "containerDirective", (node) => {
      if (node.name !== "steps") return;
      const lists = node.children.filter((c) => c.type === "list" && c.ordered);
      if (lists.length !== 1 || node.children.length !== 1) {
        file.fail("A :::steps block holds one numbered list and nothing else.", node);
      }
      node.data = { hName: "div", hProperties: { className: ["steps"] } };
    });
  };
}
