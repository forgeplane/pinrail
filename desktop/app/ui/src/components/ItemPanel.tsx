// A work item in the side panel beside the list: its title and description,
// edited in place; where it stands, its claim, repository and parent; what
// it depends on and blocks; its sub-items, reviews and sessions; and its
// comments with what happened around them, in one timeline.

import {
  Check,
  Copy,
  CornerDownRight,
  ExternalLink,
  FileText,
  FolderGit2,
  Link2,
  MessageCircleQuestion,
  MessagesSquare,
  Pencil,
  Trash2,
  X,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { Link } from "react-router";
import { ApiError, api } from "../api/client";
import type { ItemComment, ItemDetail, ItemStatus, WorkItem } from "../api/types";
import { age, stamp } from "../lib/format";
import { MOD, modalOpen } from "../lib/keys";
import { reviewPath } from "../lib/url";
import { useLive } from "../state/live";
import { useSettings } from "../state/settings";
import { useToast } from "../state/toasts";
import { AgentIcon } from "./AgentIcon";
import { ComposeField } from "./ComposeField";
import { Activity } from "./ItemActivity";
import { Markdown, useFileUrl } from "./ItemMarkdown";
import { CancelItemDialog, openSubItems } from "./CancelItemDialog";
import { DeleteItemDialog } from "./DeleteItemDialog";
import { DependencyGraph } from "./DependencyGraph";
import { ItemStatusIcon, STATUS_LABEL } from "./ItemStatusIcon";
import { Select } from "./Select";
import { sessionPath } from "./Thread";
import { Tooltip } from "./Tooltip";

const reason = (error: unknown) => (error instanceof ApiError ? error.message : "Pinrail could not be reached");

export function ItemPanel({
  itemKey,
  items,
  repos,
  onOpen,
  onClose,
  onChanged,
}: {
  itemKey: string;
  /** the items the pickers offer */
  items: WorkItem[];
  /** the repositories an item can move to */
  repos: string[];
  onOpen: (key: string) => void;
  onClose: () => void;
  /** the item changed here, for the list to show it */
  onChanged: () => void;
}) {
  const live = useLive();
  const toast = useToast();
  const { settings } = useSettings();
  const [item, setItem] = useState<ItemDetail | null>(null);
  const [missing, setMissing] = useState(false);
  const [reloads, setReloads] = useState(0);
  const [editingTitle, setEditingTitle] = useState(false);
  const [editingBody, setEditingBody] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const fileUrl = useFileUrl(item?.key);

  useEffect(() => {
    let cancelled = false;
    api
      .item(itemKey)
      .then((d) => {
        if (cancelled) return;
        setItem(d);
        setMissing(false);
        // an old key, or a number moved: the address follows the item
        if (d.key !== itemKey) onOpen(d.key);
      })
      .catch((e) => !cancelled && e instanceof ApiError && e.status === 404 && setMissing(true));
    return () => {
      cancelled = true;
    };
  }, [itemKey, live.lastNotice, reloads, onOpen]);

  /** A change the person makes, then shown here and in the list. */
  const change = useCallback(
    async (body: Record<string, unknown>) => {
      if (!item) return false;
      try {
        const done = await api.editItem(item.key, body);
        setItem(done.item);
        if (done.item.key !== item.key) onOpen(done.item.key);
        onChanged();
        return true;
      } catch (error) {
        toast(reason(error), "danger");
        return false;
      }
    },
    [item, onOpen, onChanged, toast],
  );

  // Esc closes, T edits the title, E the description
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey || modalOpen()) return;
      const el = event.target as HTMLElement | null;
      if (
        el &&
        (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable)
      )
        return;
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      } else if (event.key === "t" || event.key === "T") {
        event.preventDefault();
        setEditingTitle(true);
      } else if (event.key === "e" || event.key === "E") {
        event.preventDefault();
        setEditingBody(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  if (missing)
    return (
      <div className="item-panel" data-testid="item-panel">
        <div className="item-panel-top">
          <span className="item-panel-key mono">{itemKey}</span>
          <span className="grow" />
          <PanelButton label="Close" onClick={onClose} icon={<X size={15} />} />
        </div>
        <p className="dim">This work item is no longer here.</p>
      </div>
    );
  if (!item) return <div className="item-panel" data-testid="item-panel" />;

  const others = items.filter((i) => i.key !== item.key);
  const copy = async () => {
    const text = `# ${item.key} ${item.title}\n\n${item.body}`.trim() + "\n";
    try {
      await navigator.clipboard.writeText(text);
      toast(`Copied ${item.key} as Markdown`);
    } catch {
      toast("The item could not be copied", "danger");
    }
  };

  return (
    <div className="item-panel" data-testid="item-panel" aria-label={`${item.key} ${item.title}`}>
      <div className="item-panel-top">
        <span className="item-panel-key mono">{item.key}</span>
        <span className="grow" />
        <PanelButton label="Copy as Markdown" onClick={() => void copy()} icon={<Copy size={14} />} />
        {item.link ? (
          <PanelButton
            label="Open the link"
            onClick={() => window.open(item.link ?? "", "_blank")}
            icon={<ExternalLink size={14} />}
          />
        ) : null}
        <PanelButton label="Delete…" onClick={() => setDeleting(true)} icon={<Trash2 size={14} />} />
        <PanelButton label="Close" onClick={onClose} icon={<X size={15} />} />
      </div>
      {item.ancestors.length ? (
        <nav className="item-crumbs" aria-label="Part of">
          {item.ancestors.map((a) => (
            <button key={a.key} type="button" className="item-crumb" onClick={() => onOpen(a.key)}>
              <span className="mono">{a.key}</span> {a.title}
            </button>
          ))}
        </nav>
      ) : null}

      <EditableTitle
        title={item.title}
        editing={editingTitle}
        onEdit={setEditingTitle}
        onSave={(title) => change({ title })}
      />

      <div className="item-pills" role="group" aria-label="Properties">
        <Select
          id="item-status"
          label="Status"
          value={item.status}
          onChange={(v) =>
            v === "cancelled" && openSubItems(item) > 0 ? setCancelling(true) : void change({ status: v })
          }
          options={(["open", "in_progress", "done", "cancelled"] as ItemStatus[]).map((s) => ({
            value: s,
            label: STATUS_LABEL[s],
            icon: <ItemStatusIcon status={s} decorative />,
          }))}
        />
        {item.claim ? (
          <span className="item-claimant">
            {item.claim.agent === "person" ? (
              "by you"
            ) : (
              <>
                <AgentIcon requestedBy={item.claim.agent} size={13} /> {item.claim.agent}
              </>
            )}
          </span>
        ) : null}
        {item.status === "in_progress" ? (
          <button type="button" className="item-pill" onClick={() => void change({ claim: "release" })}>
            Release
          </button>
        ) : item.status === "open" ? (
          <button type="button" className="item-pill" onClick={() => void change({ claim: "take" })}>
            Claim
          </button>
        ) : null}
        <Select
          id="item-repo"
          searchable
          label="Repository"
          icon={<FolderGit2 size={13} />}
          value={item.repo}
          onChange={(v) => void change({ repo: v })}
          options={[...new Set([item.repo, ...repos])].sort().map((r) => ({ value: r, label: r }))}
        />
        <Select
          id="item-parent"
          searchable
          label="Parent"
          icon={<CornerDownRight size={13} />}
          value={item.parent ?? ""}
          onChange={(v) => void change({ parent: v || null })}
          options={[
            { value: "", label: "No parent" },
            ...others.map((o) => ({ value: o.key, label: `${o.key} ${o.title}` })),
            ...(item.parent && !others.some((o) => o.key === item.parent)
              ? [{ value: item.parent, label: item.parent }]
              : []),
          ]}
        />
        <EditableLink link={item.link} onSave={(link) => change({ link })} />
      </div>
      <p className="item-created" data-testid="item-created" title={stamp(item.created_at)}>
        {item.created_by === "person" ? null : <AgentIcon requestedBy={item.created_by} size={12} />}
        Added by {item.created_by === "person" ? "you" : item.created_by} · {age(item.created_at)} ago
      </p>

      <section className="item-section">
        <h3>
          Description
          {!editingBody ? (
            <button type="button" className="item-section-action" onClick={() => setEditingBody(true)}>
              <Pencil size={11} /> Edit
            </button>
          ) : null}
        </h3>
        {editingBody ? (
          <EditableBody
            body={item.body}
            onCancel={() => setEditingBody(false)}
            onSave={async (body) => {
              if (await change({ body })) setEditingBody(false);
            }}
          />
        ) : item.body.trim() ? (
          <Markdown text={item.body} files={item.files} fileUrl={fileUrl} />
        ) : (
          <p className="dim">No description yet.</p>
        )}
        {item.files.length ? (
          <ul className="item-files">
            {item.files.map((f) => (
              <li key={f.name}>
                <a href={fileUrl(f.name) ?? "#"} target="_blank" rel="noreferrer">
                  <FileText size={12} /> {f.name}
                </a>
              </li>
            ))}
          </ul>
        ) : null}
      </section>

      <section className="item-section" aria-label="Dependencies">
        <h3>
          Dependencies
          <AddPicker
            label="Add a dependency"
            options={others.filter((o) => !item.depends_on.some((d) => d.key === o.key))}
            onPick={(key) => void change({ depends_on: [key] })}
          />
        </h3>
        {item.depends_on.length || item.blocks.length ? (
          <DependencyGraph
            item={item}
            dependsOn={item.depends_on}
            blocks={item.blocks}
            onOpen={onOpen}
            onRemove={(key) => void change({ no_depends_on: [key] })}
          />
        ) : (
          <p className="dim">It depends on nothing, and nothing depends on it.</p>
        )}
        {item.depends_on
          .filter((d) => d.status === "cancelled")
          .map((d) => (
            <p key={d.key} className="item-warning" data-testid="cancelled-dependency">
              {d.key} was cancelled: the work it was to do will not happen.
            </p>
          ))}
      </section>

      <section className="item-section" aria-label="Sub-items">
        <h3>Sub-items</h3>
        {item.sub_items_list.length ? (
          <ul className="item-related">
            {item.sub_items_list.map((s) => (
              <li key={s.key}>
                <button type="button" onClick={() => onOpen(s.key)}>
                  <ItemStatusIcon status={s.status} />
                  <span className="mono">{s.key}</span> {s.title}
                </button>
              </li>
            ))}
          </ul>
        ) : null}
        <AddSubItem
          onAdd={async (title) => {
            try {
              await api.addItem({ repo: item.repo, title, parent: item.key });
              setReloads((n) => n + 1);
              onChanged();
              return true;
            } catch (error) {
              toast(reason(error), "danger");
              return false;
            }
          }}
        />
      </section>

      {item.reviews.length ? (
        <section className="item-section" aria-label="Reviews">
          <h3>Reviews</h3>
          <ul className="item-related">
            {item.reviews.map((r) => (
              <li key={r.id}>
                <Link
                  to={
                    // a question opens in its session, where it is answered
                    r.kind === "question" && r.requested_by && r.session
                      ? sessionPath({ agent: r.requested_by, id: r.session })
                      : reviewPath(r.id, settings.inbox.open_in)
                  }
                >
                  {r.kind === "question" ? <MessageCircleQuestion size={13} /> : <Check size={13} />}
                  <span className="item-related-title">{r.title}</span>
                  <span className={`item-review-status is-${r.status}`}>{r.status}</span>
                  <span className="dim">{r.kind === "question" ? "question" : r.plugin}</span>
                </Link>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {item.sessions.length ? (
        <section className="item-section" aria-label="Sessions">
          <h3>Sessions</h3>
          <ul className="item-related">
            {item.sessions.map((s) => (
              <li key={`${s.agent}/${s.session}`}>
                <Link to={sessionPath({ agent: s.agent, id: s.session })}>
                  <AgentIcon requestedBy={s.agent} size={13} />
                  <span className="mono dim">{s.session.slice(0, 8)}</span>
                  <span className="item-related-title">{s.status ?? s.agent}</span>
                </Link>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      <section className="item-section" aria-label="Comments and activity">
        <h3>
          <MessagesSquare size={12} /> Comments and activity
        </h3>
        <Activity
          entries={item.timeline}
          renderComment={(entry) => (
            <CommentCard
              key={entry.id}
              comment={entry}
              files={item.files}
              fileUrl={fileUrl}
              onEdit={async (body) => {
                try {
                  await api.editComment(item.key, entry.id, body);
                  setReloads((n) => n + 1);
                  return true;
                } catch (error) {
                  toast(reason(error), "danger");
                  return false;
                }
              }}
              onDelete={async () => {
                try {
                  await api.deleteComment(item.key, entry.id);
                  setReloads((n) => n + 1);
                } catch (error) {
                  toast(reason(error), "danger");
                }
              }}
            />
          )}
          field={
            <CommentField
              onPost={async (body) => {
                try {
                  await api.commentItem(item.key, body);
                  setReloads((n) => n + 1);
                  onChanged();
                  return true;
                } catch (error) {
                  toast(reason(error), "danger");
                  return false;
                }
              }}
            />
          }
        />
      </section>

      {cancelling ? (
        <CancelItemDialog
          item={item}
          onClose={() => setCancelling(false)}
          onConfirm={() => {
            setCancelling(false);
            void change({ status: "cancelled" });
          }}
        />
      ) : null}
      {deleting ? (
        <DeleteItemDialog
          item={item}
          onClose={() => setDeleting(false)}
          onDone={() => {
            setDeleting(false);
            onChanged();
            onClose();
          }}
        />
      ) : null}
    </div>
  );
}

function PanelButton({ label, onClick, icon }: { label: string; onClick: () => void; icon: React.ReactNode }) {
  return (
    <Tooltip label={label} side="bottom">
      <button type="button" className="chrome-button item-panel-button" aria-label={label} onClick={onClick}>
        {icon}
      </button>
    </Tooltip>
  );
}

/** The title, which a click or T turns into a field: Enter saves, Esc leaves. */
function EditableTitle({
  title,
  editing,
  onEdit,
  onSave,
}: {
  title: string;
  editing: boolean;
  onEdit: (on: boolean) => void;
  onSave: (title: string) => Promise<boolean>;
}) {
  const [draft, setDraft] = useState(title);
  const field = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (editing) {
      setDraft(title);
      field.current?.focus();
      field.current?.select();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the title as it was when editing began
  }, [editing]);
  if (!editing)
    return (
      <h2 className="item-panel-title">
        <button type="button" onClick={() => onEdit(true)} title="Edit the title (T)">
          {title}
        </button>
      </h2>
    );
  const save = async () => {
    if (!draft.trim() || draft.trim() === title) return onEdit(false);
    if (await onSave(draft.trim())) onEdit(false);
  };
  return (
    <input
      ref={field}
      className="item-title-field"
      aria-label="Title"
      value={draft}
      autoComplete="off"
      autoCorrect="off"
      spellCheck={false}
      onChange={(e) => setDraft(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          void save();
        } else if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          onEdit(false);
        }
      }}
      onBlur={() => onEdit(false)}
    />
  );
}

/** The description as Markdown source: ⌘Enter saves, Esc leaves. */
function EditableBody({
  body,
  onSave,
  onCancel,
}: {
  body: string;
  onSave: (body: string) => Promise<void>;
  onCancel: () => void;
}) {
  const [draft, setDraft] = useState(body);
  const field = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    field.current?.focus();
  }, []);
  return (
    <div className="item-body-edit">
      <textarea
        ref={field}
        aria-label="Description"
        value={draft}
        rows={8}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            void onSave(draft);
          } else if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            onCancel();
          }
        }}
      />
      <div className="item-edit-actions">
        <button type="button" className="chrome-button" onClick={onCancel}>
          Cancel
        </button>
        <button type="button" className="chrome-button button-primary" onClick={() => void onSave(draft)}>
          Save
        </button>
      </div>
    </div>
  );
}

/** The link, or a field to give one; an empty one removes it. */
function EditableLink({ link, onSave }: { link: string | null; onSave: (link: string | null) => Promise<boolean> }) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(link ?? "");
  const edit = () => {
    setDraft(link ?? "");
    setEditing(true);
  };
  if (!editing)
    return link ? (
      <button type="button" className="item-pill item-link" aria-label="Change the link" title={link} onClick={edit}>
        <Link2 size={13} aria-hidden="true" />
        <span className="item-pill-text">{link.replace(/^https?:\/\//, "")}</span>
      </button>
    ) : (
      <button type="button" className="item-pill is-empty" onClick={edit}>
        <Link2 size={13} aria-hidden="true" />
        Add link
      </button>
    );
  return (
    <input
      autoFocus
      className="item-link-field"
      aria-label="Link"
      placeholder="https://"
      value={draft}
      onChange={(e) => setDraft(e.target.value)}
      onKeyDown={async (e: ReactKeyboardEvent<HTMLInputElement>) => {
        if (e.key === "Enter") {
          e.preventDefault();
          if (await onSave(draft.trim() || null)) setEditing(false);
        } else if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          setEditing(false);
        }
      }}
      onBlur={() => setEditing(false)}
    />
  );
}

/** A searchable menu of items to add, behind a small button. */
function AddPicker({ label, options, onPick }: { label: string; options: WorkItem[]; onPick: (key: string) => void }) {
  return (
    <span className="item-section-action item-add-picker">
      <Select
        id="item-add-dependency"
        searchable
        label={label}
        value=""
        onChange={(v) => v && onPick(v)}
        options={[
          { value: "", label: "Add…" },
          ...options.map((o) => ({ value: o.key, label: `${o.key} ${o.title}` })),
        ]}
      />
    </span>
  );
}

/** A field that adds a sub-item: Enter adds it, and the field stays for the next. */
function AddSubItem({ onAdd }: { onAdd: (title: string) => Promise<boolean> }) {
  const [title, setTitle] = useState("");
  return (
    <input
      className="item-sub-field"
      aria-label="Add a sub-item"
      placeholder="Add a sub-item…"
      value={title}
      autoComplete="off"
      autoCorrect="off"
      spellCheck={false}
      onChange={(e) => setTitle(e.target.value)}
      onKeyDown={async (e) => {
        if (e.key === "Enter" && title.trim()) {
          e.preventDefault();
          if (await onAdd(title.trim())) setTitle("");
        } else if (e.key === "Escape") {
          (e.target as HTMLInputElement).blur();
        }
      }}
    />
  );
}

function CommentCard({
  comment,
  files,
  fileUrl,
  onEdit,
  onDelete,
}: {
  comment: ItemComment;
  files: { name: string; media_type: string }[];
  fileUrl: (n: string) => string | null;
  onEdit: (body: string) => Promise<boolean>;
  onDelete: () => Promise<void>;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(comment.body);
  const mine = comment.author === "person";
  return (
    <div className={`item-comment ${mine ? "is-mine" : ""}`} data-testid="item-comment">
      <div className="item-comment-head">
        {mine ? <span className="item-me">You</span> : <AgentIcon requestedBy={comment.author} size={13} />}
        <b>{mine ? "You" : comment.author}</b>
        <time dateTime={comment.created_at} title={stamp(comment.created_at)}>
          {age(comment.created_at)} ago
        </time>
        {comment.edited_at ? <span className="dim">edited</span> : null}
        <span className="grow" />
        {mine && !editing ? (
          <>
            <button type="button" className="item-section-action" onClick={() => setEditing(true)}>
              Edit
            </button>
            <button type="button" className="item-section-action" onClick={() => void onDelete()}>
              Delete
            </button>
          </>
        ) : null}
      </div>
      {editing ? (
        <div className="item-body-edit">
          <textarea
            aria-label="Comment"
            value={draft}
            rows={3}
            autoFocus
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={async (e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                if (await onEdit(draft)) setEditing(false);
              } else if (e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                setEditing(false);
              }
            }}
          />
        </div>
      ) : (
        <Markdown text={comment.body} files={[...files, ...comment.files]} fileUrl={fileUrl} />
      )}
      {comment.files.length ? (
        <ul className="item-files">
          {comment.files.map((f) => (
            <li key={f.name}>
              <a href={fileUrl(f.name) ?? "#"} target="_blank" rel="noreferrer">
                <FileText size={12} /> {f.name}
              </a>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

/** The field at the end of the timeline: ⌘Enter posts the comment. */
function CommentField({ onPost }: { onPost: (body: string) => Promise<boolean> }) {
  const [draft, setDraft] = useState("");
  const post = async () => {
    if (draft.trim() && (await onPost(draft.trim()))) setDraft("");
  };
  return (
    <ComposeField
      className="item-comment-field"
      value={draft}
      onChange={setDraft}
      label="Add a comment"
      placeholder="Add a comment…"
      onSubmit={() => void post()}
      submitKey="mod-enter"
      keys={`${MOD === "⌘" ? "⌘" : "Ctrl+"}Enter to comment`}
      send={{ label: "Comment", keys: [MOD, "Enter"], disabled: !draft.trim() }}
    />
  );
}
