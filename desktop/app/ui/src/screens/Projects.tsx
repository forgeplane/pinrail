// The person's projects: named sets of repositories that the inbox and the
// sidebar can group by. One list, as the inbox groups by repository: a
// heading per project with what waits in it, its repositories as rows, and
// the repositories in no project last. A repository moves by dragging it
// under another heading, or from its menu.

import {
  ChevronDown,
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  FolderGit2,
  GripVertical,
  Inbox,
  LayoutGrid,
  MessagesSquare,
  MoreHorizontal,
  Pencil,
  Plus,
  Trash2,
  X,
} from "lucide-react";
import { useCallback, useEffect, useId, useRef, useState, type DragEvent, type FormEvent } from "react";
import { useLocation, useNavigate } from "react-router";
import { ApiError, api } from "../api/client";
import type { Project, ProjectRepo, ProjectsListing } from "../api/types";
import { PopoverMenu, type MenuEntry } from "../components/PopoverMenu";
import { Tooltip } from "../components/Tooltip";
import { PROJECT_COLORS, ProjectAvatar, colorOf, type ProjectColor } from "../components/ProjectAvatar";
import { iconInput } from "../lib/projectAvatar";
import { squarePng } from "../lib/projectImage";
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
  { kind: "new"; repos: string[] } | { kind: "edit"; project: Project } | { kind: "delete"; project: Project };

export function Projects() {
  const live = useLive();
  const toast = useToast();
  const navigate = useNavigate();
  const location = useLocation();
  const { settings: prefs } = useSettings();
  const sessionsOn = prefs.labs.sessions;
  const [listing, setListing] = useState<ProjectsListing | null>(null);
  const [menu, setMenu] = useState<Menu | null>(null);
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [dropOn, setDropOn] = useState<string | null>(null);
  const [folded, setFolded] = useState<string[]>([]);
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
  // the command palette's New project opens the dialog here, once
  useEffect(() => {
    if ((location.state as { newProject?: boolean } | null)?.newProject) {
      setDialog({ kind: "new", repos: [] });
      navigate(location.pathname, { replace: true, state: null });
    }
  }, [location.state, location.pathname, navigate]);
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
        {
          label: "Open in the inbox",
          icon: Inbox,
          onPick: () => navigate(`/?project=${encodeURIComponent(project.id)}`),
        },
        ...(sessionsOn
          ? [
              {
                label: "Open its sessions",
                icon: MessagesSquare,
                onPick: () => navigate(`/sessions?project=${encodeURIComponent(project.id)}`),
              } as MenuEntry,
            ]
          : []),
        { kind: "separator" },
        { label: "Edit…", icon: Pencil, onPick: () => setDialog({ kind: "edit", project }) },
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
  // every group on the page, the repositories in no project included
  const groupKeys = [...shown.map((p) => p.id), ...(shownLoose.length ? [LOOSE] : [])];
  const allFolded = groupKeys.length > 0 && groupKeys.every((key) => folded.includes(key));

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
        {actions ? (
          <ProjectAvatar project={actions} size={20} />
        ) : (
          <Icon size={14} className="project-group-icon" aria-hidden />
        )}
        <h2>{title}</h2>
        {actions?.description ? (
          <span className="project-group-description" title={actions.description}>
            {actions.description}
          </span>
        ) : null}
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
            autoComplete="off"
            autoCorrect="off"
            autoCapitalize="off"
            spellCheck={false}
            type="search"
            aria-label="Filter projects and repositories"
            placeholder="Filter projects and repositories"
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
          />
          <Tooltip label={allFolded ? "Expand all" : "Collapse all"} side="bottom">
            <button
              type="button"
              className="chrome-button"
              aria-label={allFolded ? "Expand all" : "Collapse all"}
              disabled={groupKeys.length === 0}
              onClick={() => setFolded(allFolded ? [] : groupKeys)}
            >
              {allFolded ? <ChevronsUpDown size={14} /> : <ChevronsDownUp size={14} />}
            </button>
          </Tooltip>
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
                {project.repos.length === 0 ? <li className="project-empty">Drop a repository here</li> : null}
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
          repos={dialog.repos}
          onSave={async ({ name, description, color, emoji, image }) => {
            const made = await api.createProject(name, { description, color, emoji }, dialog.repos);
            if (image) await api.setProjectImage(made.id, image);
            setDialog(null);
          }}
          onClose={() => setDialog(null)}
        />
      ) : null}
      {dialog?.kind === "edit" ? (
        <ProjectDialog
          title="Edit project"
          action="Save"
          project={dialog.project}
          onSave={async ({ image, ...change }) => {
            await api.updateProject(dialog.project.id, change);
            if (image !== undefined) await api.setProjectImage(dialog.project.id, image);
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

/** Asks for a project's name and, optionally, a line on what it is: for a
 *  new project, or to change one. A new project started from a
 *  repository's menu says it begins with that repository. */
function ProjectDialog({
  title,
  action,
  project,
  repos,
  onSave,
  onClose,
}: {
  title: string;
  action: string;
  /** the project to change; none for a new one */
  project?: Project;
  /** the repositories a new project begins with */
  repos?: string[];
  /** `image` is a new PNG, `null` to remove the project's, or left out to keep it */
  onSave: (fields: {
    name: string;
    description: string;
    color: string;
    emoji: string;
    image?: Blob | null;
  }) => Promise<void>;
  onClose: () => void;
}) {
  const [name, setName] = useState(project?.name ?? "");
  const [description, setDescription] = useState(project?.description ?? "");
  // a new project starts with a colour of its own, which the person can change
  const [color, setColor] = useState<ProjectColor>(() =>
    project ? colorOf(project) : PROJECT_COLORS[Math.floor(Math.random() * PROJECT_COLORS.length)],
  );
  const [emoji, setEmoji] = useState(project?.emoji ?? "");
  const nameId = useId();
  // a PNG chosen here, with its preview, or "removed" for the project's own
  const [image, setImage] = useState<{ png: Blob; url: string } | "removed" | null>(null);
  const picker = useRef<HTMLInputElement>(null);
  useEffect(() => () => void (image && image !== "removed" && URL.revokeObjectURL(image.url)), [image]);
  const choose = async (file: File | undefined) => {
    if (!file) return;
    try {
      const png = await squarePng(file);
      setImage({ png, url: URL.createObjectURL(png) });
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "The image could not be read.");
    }
  };
  const hasImage = image === "removed" ? false : !!image || !!project?.image;
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
      await onSave({
        name,
        description,
        color,
        emoji,
        image: image === "removed" ? null : image ? image.png : undefined,
      });
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
        <div className="feedback-field">
          <label htmlFor={nameId}>Name</label>
          {/* the avatar as it will show, beside the name it takes its letter from */}
          <span className="project-name-row">
            {/* a PNG dropped on the preview is chosen as the image */}
            <span
              className="project-avatar-drop"
              onDragOver={(event) => {
                if (event.dataTransfer.types.includes("Files")) event.preventDefault();
              }}
              onDrop={(event) => {
                event.preventDefault();
                void choose(event.dataTransfer.files[0]);
              }}
            >
              <ProjectAvatar
                project={{ id: project?.id ?? "", name: name || "?", color, emoji, image: project?.image }}
                size={32}
                src={image === "removed" ? "" : image?.url}
              />
            </span>
            <input
              id={nameId}
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
          </span>
        </div>
        <label className="feedback-field">
          <span>Description (optional)</span>
          <input
            className="feedback-input"
            value={description}
            maxLength={280}
            placeholder="Such as The app, its plugins and its website"
            onChange={(event) => {
              setDescription(event.target.value);
              setError(null);
            }}
          />
        </label>
        <div className="feedback-field">
          <span id="project-colour">Colour</span>
          <div className="project-swatches" role="radiogroup" aria-labelledby="project-colour">
            {PROJECT_COLORS.map((c) => (
              <button
                key={c}
                type="button"
                role="radio"
                aria-checked={color === c}
                aria-label={c[0].toUpperCase() + c.slice(1)}
                className={`project-swatch ${color === c ? "is-on" : ""}`}
                style={{ background: `var(--project-${c})` }}
                onClick={() => setColor(c)}
              />
            ))}
          </div>
        </div>
        <label className="feedback-field">
          <span>Icon (optional)</span>
          <input
            className="feedback-input"
            value={emoji}
            maxLength={8}
            placeholder="An emoji, or up to three letters"
            onChange={(event) => {
              setEmoji(iconInput(event.target.value));
              setError(null);
            }}
          />
          <span className="project-field-hint">Without one, the avatar shows the name's first letter.</span>
        </label>
        <div className="feedback-field">
          <span>Image (optional)</span>
          <span className="project-image-row">
            <button type="button" className="chrome-button" onClick={() => picker.current?.click()}>
              Choose PNG…
            </button>
            {hasImage ? (
              <button type="button" className="chrome-button" onClick={() => setImage("removed")}>
                Remove image
              </button>
            ) : null}
            <input
              ref={picker}
              type="file"
              accept="image/png"
              hidden
              aria-label="Image"
              onChange={(event) => {
                void choose(event.target.files?.[0]);
                event.target.value = "";
              }}
            />
          </span>
          <span className="project-field-hint">
            A PNG, which takes the place of the icon. Its middle square is kept. You can also drop one on the avatar.
          </span>
        </div>
        {repos?.length ? (
          <p className="dim project-dialog-note">It starts with {repos.join(", ")}. Drag more repositories under it.</p>
        ) : repos ? (
          <p className="dim project-dialog-note">Add repositories by dragging them under it.</p>
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
