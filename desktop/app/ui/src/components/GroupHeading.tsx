// A group's heading in the inbox or on the Sessions page: the arrow that
// folds it, the project's avatar or the repository's icon, its name and how
// many it holds. It goes inside the group's <summary>.

import { ChevronDown, FolderGit2 } from "lucide-react";
import type { ProjectRef } from "../api/types";
import { ProjectAvatar } from "./ProjectAvatar";

export function GroupHeading({
  groupKey,
  project,
  count,
}: {
  /** `project:<id>`, a repository, or "" for none */
  groupKey: string;
  /** the project, for a project's group */
  project?: ProjectRef | null;
  count: number;
}) {
  const isProject = groupKey.startsWith("project:") && !!project;
  return (
    <>
      {/* points down while the group is open, and right once it is folded */}
      <ChevronDown size={13} className="inbox-group-fold" aria-hidden />
      {isProject ? (
        <ProjectAvatar project={project} size={20} className="inbox-group-icon" />
      ) : (
        <FolderGit2 size={13} className="inbox-group-icon" aria-label="Repository" />
      )}
      <strong data-testid="inbox-group-name">{isProject ? project.name : groupKey || "No repository"}</strong>
      <span>{count}</span>
    </>
  );
}
