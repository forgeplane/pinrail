// The person's projects: named sets of repositories that the inbox and the
// sidebar can group by. One list, as the inbox groups by repository: a
// heading per project with what waits in it, its repositories as rows, and
// the repositories in no project last. A repository moves by dragging it
// under another heading, or from its menu.

import {
  ChevronDown,
  ChevronRight,
  FolderGit2,
  GripVertical,
  LayoutGrid,
  MoreHorizontal,
  Pencil,
  Plus,
  Trash2,
  X,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState, type DragEvent, type FormEvent } from "react";
import { ApiError, api } from "../api/client";
import type { Project, ProjectRepo, ProjectsListing } from "../api/types";
import { PopoverMenu, type MenuEntry } from "../components/PopoverMenu";
import { age, stamp } from "../lib/format";
import { useLive } from "../state/live";
import { useSettings } from "../state/settings";
import { useToast } from "../state/toasts";

/** The type of a repository's name on a drag. */
const DRAGGED = "application/x-pinrail-repo";
/** The key of the repositories in no project, among the groups. */
const LOOSE = "-";

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** Why a request was refused, as the person should read it. */
const reason = (error: unknown) =>
  error instanceof ApiError ? (error.violations[0]?.message ?? error.message) : "The app could not be reached.";

type Menu = { anchor: HTMLElement; label: string; entries: MenuEntry[]; align?: "left" | "right" };
type Dialog =
  { kind: "new"; repos: string[] } | { kind: "rename"; project: Project } | { kind: "delete"; project: Project };

export function Projects() {
  const live = useLive();
  const toast = useToast();
  const { settings: prefs } = useSettings();
  const sessionsOn = prefs.labs.sessions;
  const [listing, setListing] = useState<ProjectsListing | null>(null);
  const [menu, setMenu] = useState<Menu | null>(null);
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [dropOn, setDropOn] = useState<string | null>(null);
  const [folded, setFolded] = useState<string[]>([]);
  const [adding, setAdding] = useState<string | null>(null);
  const [filter, setFilter] = useState("");

  // what waits and when it last moved change with every event
  useEffect(() => {
    let cancelled = false;
    api
      .projects()
      .then((l) => !cancelled && setListing(l))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [live.lastNotice, live.projectsVersion]);
  // a menu opened on a project or a repository that changed since is stale
  useEffect(() => setMenu(null), [live.projectsVersion]);

  // by name, as the core lists them
  const projects = listing?.projects ?? [];
  const loose = listing?.loose ?? [];
  const words = filter.trim().toLowerCase();
  const matches = (text: string) => !words || text.toLowerCase().includes(words);
  // a project shown whole when its name matches, otherwise with the
  // repositories that do
  const shown = projects
    .map((p) => (matches(p.name) ? p : { ...p, repos: p.repos.filter((r) => matches(r.repo)) }))
    .filter((p) => !words || matches(p.name) || p.repos.length);
  const shownLoose = loose.filter((r) => matches(r.repo));

  const move = useCallback(
    async (repo: string, project: string | null) => {
      try {
        await api.moveRepo(repo, project);
      } catch (error) {
        toast(reason(error), "danger");
      }
    },
    [toast],
  );

  const projectOf = (repo: string) => projects.find((p) => p.repos.some((r) => r.repo === repo)) ?? null;

  const repoMenu = (repo: string, anchor: HTMLElement) => {
    const current = projectOf(repo);
    const targets = projects.filter((p) => p.id !== current?.id);
    setMenu({
      anchor,
      label: `Move ${repo}`,
      align: "right",
      entries: [
        { kind: "heading", label: `Move ${repo} to` },
        ...targets.map<MenuEntry>((p) => ({ label: p.name, icon: LayoutGrid, onPick: () => move(repo, p.id) })),
        ...(targets.length ? [{ kind: "separator" } as MenuEntry] : []),
        { label: "New project…", icon: Plus, onPick: () => setDialog({ kind: "new", repos: [repo] }) },
        ...(current
          ? [{ label: `Remove from ${current.name}`, icon: X, onPick: () => move(repo, null) } as MenuEntry]
          : []),
      ],
    });
  };

  const projectMenu = (project: Project, anchor: HTMLElement) =>
    setMenu({
      anchor,
      label: project.name,
      align: "right",
      entries: [
        { label: "Rename…", icon: Pencil, onPick: () => setDialog({ kind: "rename", project }) },
        { kind: "separator" },
        { label: "Delete project…", icon: Trash2, danger: true, onPick: () => setDialog({ kind: "delete", project }) },
      ],
    });

  // a repository dropped on a project's group goes in that project, and
  // dropped on the last group, in none
  const dropTarget = (key: string, project: string | null) => ({
    onDragOver: (event: DragEvent) => {
      if (!event.dataTransfer.types.includes(DRAGGED)) return;
      event.preventDefault();
      event.dataTransfer.dropEffect = "move";
      setDropOn(key);
    },
    onDragLeave: (event: DragEvent) => {
      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDropOn(null);
    },
    onDrop: (event: DragEvent) => {
      event.preventDefault();
      setDropOn(null);
      const repo = event.dataTransfer.getData(DRAGGED);
      if (repo && projectOf(repo)?.id !== (project ?? undefined)) void move(repo, project);
    },
  });

  const toggle = (key: string) =>
    setFolded((now) => (now.includes(key) ? now.filter((k) => k !== key) : [...now, key]));

  const repoRow = (repo: ProjectRepo) => (
    <li
      key={repo.repo}
      className="project-repo"
      draggable
      data-testid="project-repo"
      onDragStart={(event) => {
        event.dataTransfer.setData(DRAGGED, repo.repo);
        event.dataTransfer.effectAllowed = "move";
      }}
      onDragEnd={() => setDropOn(null)}
    >
      <GripVertical size={13} className="project-grip" aria-hidden />
      <FolderGit2 size={14} className="project-repo-icon" aria-hidden />
      <span className="project-repo-name">{repo.repo}</span>
      <span className={`project-col ${repo.waiting ? "is-waiting" : "faint"}`}>
        {repo.waiting ? `${repo.waiting} waiting` : "—"}
      </span>
      {sessionsOn ? (
        <span className="project-col dim">{repo.sessions ? plural(repo.sessions, "session", "sessions") : ""}</span>
      ) : null}
      <span className="project-col faint" title={stamp(repo.last_at)}>
        {repo.last_at ? `${age(repo.last_at)} ago` : ""}
      </span>
      <button
        type="button"
        className="bar-button project-row-menu"
        aria-label={`Move ${repo.repo}`}
        aria-haspopup="menu"
        onClick={(event) => repoMenu(repo.repo, event.currentTarget)}
      >
        <MoreHorizontal size={14} />
      </button>
    </li>
  );

  const head = (key: string, title: string, icon: typeof LayoutGrid, meta: (string | null)[], actions?: Project) => {
    const open = !folded.includes(key);
    const Icon = icon;
    return (
      <div className="project-group-head">
        <button
          type="button"
          className="project-fold"
          aria-expanded={open}
          aria-label={open ? `Fold ${title}` : `Unfold ${title}`}
          onClick={() => toggle(key)}
        >
          {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </button>
        <Icon size={14} className="project-group-icon" aria-hidden />
        <h2>{title}</h2>
        <span className="project-group-meta">
          {meta.filter(Boolean).map((m) => (
            <span key={m} className={m!.endsWith("waiting") && !m!.startsWith("Nothing") ? "is-waiting" : ""}>
              {m}
            </span>
          ))}
        </span>
        {actions ? (
          <span className="project-group-actions">
            <button
              type="button"
              className="bar-button"
              aria-label={`Add a repository to ${actions.name}`}
              onClick={() => {
                setFolded((now) => now.filter((k) => k !== key));
                setAdding(actions.id);
              }}
            >
              <Plus size={14} />
            </button>
            <button
              type="button"
              className="bar-button"
              aria-label={`${actions.name} actions`}
              aria-haspopup="menu"
              onClick={(event) => projectMenu(actions, event.currentTarget)}
            >
              <MoreHorizontal size={15} />
            </button>
          </span>
        ) : null}
      </div>
    );
  };

  return (
    <div className="projects-page">
      <header className="page-head">
        <div className="heading-inline">
          <h1>Projects</h1>
          {projects.length ? <span className="inbox-total">{projects.length}</span> : null}
        </div>
        <div className="inbox-controls">
          <input
            type="search"
            aria-label="Filter projects and repositories"
            placeholder="Filter projects and repositories"
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
          />
          <button
            type="button"
            className="chrome-button button-primary"
            onClick={() => setDialog({ kind: "new", repos: [] })}
          >
            <Plus size={14} /> New project
          </button>
        </div>
      </header>

      {listing && projects.length === 0 ? (
        <section className="projects-intro" role="status">
          <LayoutGrid size={22} className="projects-intro-icon" aria-hidden />
          <div>
            <h2>Group repositories into projects</h2>
            <p className="dim">
              A project puts the work of several repositories together, such as an app, its infrastructure and its
              plugins. Group the inbox and the sidebar by project to see that work in one place. Drag a repository under
              a project to move it.
            </p>
          </div>
        </section>
      ) : null}

      <div className="project-groups">
        {shown.map((project) => (
          <section
            key={project.id}
            className={`project-group ${dropOn === project.id ? "is-drop" : ""}`}
            aria-label={project.name}
            data-testid="project-group"
            {...dropTarget(project.id, project.id)}
          >
            {head(
              project.id,
              project.name,
              LayoutGrid,
              [
                project.waiting ? `${project.waiting} waiting` : "Nothing waiting",
                sessionsOn && project.sessions ? plural(project.sessions, "session", "sessions") : null,
                project.last_at ? `Updated ${age(project.last_at)} ago` : null,
              ],
              project,
            )}
            {folded.includes(project.id) ? null : (
              <ul className="project-repos" aria-label={`${project.name} repositories`}>
                {project.repos.map(repoRow)}
                {adding === project.id ? (
                  <AddRepo
                    onAdd={(repo) => move(repo, project.id)}
                    onDone={() => setAdding((a) => (a === project.id ? null : a))}
                  />
                ) : project.repos.length === 0 ? (
                  <li className="project-empty">
                    Drop a repository here, or{" "}
                    <button type="button" className="link-button" onClick={() => setAdding(project.id)}>
                      add one by name
                    </button>
                  </li>
                ) : null}
              </ul>
            )}
          </section>
        ))}

        {shownLoose.length ? (
          <section
            className={`project-group is-loose ${dropOn === LOOSE ? "is-drop" : ""}`}
            aria-label="Repositories in no project"
            {...dropTarget(LOOSE, null)}
          >
            {head(LOOSE, "In no project", FolderGit2, [plural(shownLoose.length, "repository", "repositories")])}
            {folded.includes(LOOSE) ? null : <ul className="project-repos">{shownLoose.map(repoRow)}</ul>}
          </section>
        ) : null}
      </div>

      {menu ? (
        <PopoverMenu
          anchor={menu.anchor}
          label={menu.label}
          entries={menu.entries}
          align={menu.align}
          onClose={() => setMenu(null)}
        />
      ) : null}
      {dialog?.kind === "new" ? (
        <ProjectDialog
          title="New project"
          action="Create project"
          initial=""
          choices={[
            ...loose.map((r) => ({ repo: r.repo, owner: null })),
            ...projects.flatMap((p) => p.repos.map((r) => ({ repo: r.repo, owner: p.name }))),
          ]}
          chosen={dialog.repos}
          onSave={async (name, repos) => {
            await api.createProject(name, repos);
            setDialog(null);
          }}
          onClose={() => setDialog(null)}
        />
      ) : null}
      {dialog?.kind === "rename" ? (
        <ProjectDialog
          title="Rename project"
          action="Rename"
          initial={dialog.project.name}
          onSave={async (name) => {
            await api.renameProject(dialog.project.id, name);
            setDialog(null);
          }}
          onClose={() => setDialog(null)}
        />
      ) : null}
      {dialog?.kind === "delete" ? (
        <DeleteDialog
          project={dialog.project}
          onDelete={async () => {
            try {
              await api.deleteProject(dialog.project.id);
            } catch (error) {
              toast(reason(error), "danger");
            }
            setDialog(null);
          }}
          onClose={() => setDialog(null)}
        />
      ) : null}
    </div>
  );
}

/** A row that takes a repository's name, for one nothing has sent from yet. */
function AddRepo({ onAdd, onDone }: { onAdd: (repo: string) => void; onDone: () => void }) {
  const [repo, setRepo] = useState("");
  return (
    <li className="project-add-row">
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (repo.trim()) onAdd(repo.trim());
          onDone();
        }}
      >
        <input
          autoFocus
          className="feedback-input"
          aria-label="Repository"
          placeholder="A repository, such as acme/web"
          value={repo}
          onChange={(event) => setRepo(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.stopPropagation();
              onDone();
            }
          }}
          onBlur={() => !repo.trim() && onDone()}
        />
      </form>
    </li>
  );
}

/** Asks for a project's name, to create or rename one, and for a new
 *  project, the repositories it starts with. */
function ProjectDialog({
  title,
  action,
  initial,
  choices = [],
  chosen = [],
  onSave,
  onClose,
}: {
  title: string;
  action: string;
  initial: string;
  /** the repositories to pick from, each with the project it is in */
  choices?: { repo: string; owner: string | null }[];
  /** those picked to begin with */
  chosen?: string[];
  onSave: (name: string, repos: string[]) => Promise<void>;
  onClose: () => void;
}) {
  const [name, setName] = useState(initial);
  const [picked, setPicked] = useState<string[]>(chosen);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    input.current?.select();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setSaving(true);
    try {
      await onSave(name, picked);
    } catch (e) {
      setError(reason(e));
      setSaving(false);
    }
  };

  return (
    <div className="app-dialog-backdrop" onMouseDown={onClose}>
      <form
        className="app-dialog project-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="project-dialog-title"
        onMouseDown={(event) => event.stopPropagation()}
        onSubmit={submit}
      >
        <div className="dialog-head">
          <h2 id="project-dialog-title">{title}</h2>
        </div>
        <label className="feedback-field">
          <span>Name</span>
          <input
            ref={input}
            className="feedback-input"
            value={name}
            maxLength={80}
            placeholder="Such as Pinrail"
            onChange={(event) => {
              setName(event.target.value);
              setError(null);
            }}
          />
        </label>
        {choices.length ? (
          <fieldset className="project-choices">
            <legend>Repositories (optional)</legend>
            {choices.map(({ repo, owner }) => (
              <label key={repo} className="project-choice">
                <input
                  type="checkbox"
                  checked={picked.includes(repo)}
                  onChange={() =>
                    setPicked((now) => (now.includes(repo) ? now.filter((r) => r !== repo) : [...now, repo]))
                  }
                />
                <span className="project-choice-repo">{repo}</span>
                {owner ? <span className="dim">in {owner}, moves</span> : null}
              </label>
            ))}
          </fieldset>
        ) : null}
        {error ? <p className="notice notice-danger">{error}</p> : null}
        <div className="dialog-actions">
          <button type="button" className="chrome-button" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className="chrome-button button-primary" disabled={!name.trim() || saving}>
            {action}
          </button>
        </div>
      </form>
    </div>
  );
}

/** Asks before a project is deleted, saying what stays. */
function DeleteDialog({ project, onDelete, onClose }: { project: Project; onDelete: () => void; onClose: () => void }) {
  const cancel = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    cancel.current?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);
  return (
    <div className="app-dialog-backdrop" onMouseDown={onClose}>
      <div
        className="app-dialog project-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="project-delete-title"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="dialog-head">
          <h2 id="project-delete-title">Delete {project.name}?</h2>
        </div>
        <p className="dim">
          Its repositories will be in no project. Their reviews, questions and sessions stay as they are.
        </p>
        <div className="dialog-actions">
          <button ref={cancel} type="button" className="chrome-button" onClick={onClose}>
            Cancel
          </button>
          <button type="button" className="chrome-button button-danger" onClick={onDelete}>
            Delete project
          </button>
        </div>
      </div>
    </div>
  );
}
