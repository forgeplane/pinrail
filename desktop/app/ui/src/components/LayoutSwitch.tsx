// The buttons that lay a list out grouped by project, grouped by
// repository, or as one list, in the inbox and on the Sessions page.

import { FolderGit2, LayoutGrid, List } from "lucide-react";
import type { Layout } from "../lib/grouping";
import { Tooltip } from "./Tooltip";

const CHOICES: { value: Layout; label: string; Icon: typeof List }[] = [
  { value: "project", label: "Grouped by project", Icon: LayoutGrid },
  { value: "repository", label: "Grouped by repository", Icon: FolderGit2 },
  { value: "list", label: "One list", Icon: List },
];

export function LayoutSwitch({
  value,
  onChange,
  label,
  listTip = "One list",
}: {
  value: Layout;
  onChange: (next: Layout) => void;
  /** the group's accessible name, such as "Inbox layout" */
  label: string;
  /** the one list's tooltip, which may say how it is ordered */
  listTip?: string;
}) {
  return (
    <span className="segmented inbox-layout" role="radiogroup" aria-label={label}>
      {CHOICES.map(({ value: choice, label: name, Icon }) => (
        <Tooltip key={choice} label={choice === "list" ? listTip : name} side="top">
          <button
            type="button"
            role="radio"
            aria-checked={value === choice}
            aria-label={name}
            className={value === choice ? "is-on" : ""}
            onClick={() => onChange(choice)}
          >
            <Icon size={13} />
          </button>
        </Tooltip>
      ))}
    </span>
  );
}
