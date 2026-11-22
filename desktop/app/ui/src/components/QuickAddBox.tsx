// The quick add's open box: a title field, the repository and the parent
// the item goes in, a description that takes pasted or dropped files, and
// the keys that add it. While the agent runtime is on, a button adds the
// item and starts an agent on it at once. The Work items page shows it at
// the top of its list, and the quick add dialog on any other page.

import { ChevronDown, FolderGit2, Play, Plus, X } from "lucide-react";
import { useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { api, errorMessage } from "../api/client";
import { useAttach } from "../lib/attach";
import { countFeature } from "../lib/usage";
import type { WorkItem } from "../api/types";
import { ALT, MOD, SHIFT, composing } from "../lib/keys";
import { useSettings } from "../state/settings";
import { useToast } from "../state/toasts";
import { useItemPicker } from "./ItemPicker";
import { AgentIcon } from "./AgentIcon";
import { ItemStatusIcon } from "./ItemStatusIcon";
import { KeyHints } from "./KeyHints";
import { baseName } from "./NewSession";
import { PopoverMenu } from "./PopoverMenu";
import { Select } from "./Select";
import { Tooltip } from "./Tooltip";
import { useAgentStart } from "./useAgentStart";

/** The tallest the description grows before it scrolls. */
const BODY_MAX = 240;

export type QuickAddBoxProps = {
  onClose: () => void;
  repos: string[];
  items: WorkItem[];
  defaultRepo: string | null;
  defaultParent: string | null;
  /** after an item is added: `more` keeps the box open for another, and
   *  `started` says an agent was started on it, which a toast names */
  onAdded: (item: WorkItem, more: boolean, started: boolean) => void;
  /** what Enter does, in the key hints */
  enterHint?: string;
};

/** The open quick add. It is made each time it opens, starting from the
 *  list's current place, so nothing resets it after the person starts typing. */
export function QuickAddBox({
  onClose,
  repos,
  items,
  defaultRepo,
  defaultParent,
  onAdded,
  enterHint = "Add and open",
}: QuickAddBoxProps) {
  const toast = useToast();
  const sorted = useMemo(() => [...repos].sort(), [repos]);
  const [title, setTitle] = useState("");
  const [repo, setRepo] = useState(() => defaultRepo ?? [...repos].sort()[0] ?? "");
  const [parent, setParent] = useState(() => defaultParent ?? "");
  const [busy, setBusy] = useState(false);
  const field = useRef<HTMLInputElement>(null);
  const [body, setBody] = useState("");
  const bodyField = useRef<HTMLTextAreaElement>(null);
  // the description's keys show while it has the focus
  const [inBody, setInBody] = useState(false);
  const files = useAttach({ box: bodyField, onChange: setBody, onError: (m) => toast(m, "danger") });
  const picker = useItemPicker({ box: bodyField, value: body, onChange: setBody, repo: repo || null });
  // the description grows with its text, up to a height it then scrolls past
  useLayoutEffect(() => {
    const el = bodyField.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, BODY_MAX)}px`;
  }, [body]);
  // focused before it is shown, so no key typed after C goes elsewhere
  useLayoutEffect(() => {
    field.current?.focus();
  }, []);

  const { settings } = useSettings();
  const agent = useAgentStart(repo, settings.labs.runtime);
  const [menu, setMenu] = useState<HTMLElement | null>(null);
  const startMore = useRef<HTMLButtonElement>(null);

  const parents = items.filter((i) => i.repo === repo && i.status !== "done" && i.status !== "cancelled");

  /** Adds the item; with `start`, then starts the chosen agent on it. */
  const add = async (more: boolean, start = false) => {
    if (!title.trim() || !repo || busy || files.busy) return;
    if (start && !agent.chosen) return;
    setBusy(true);
    // the folder is settled first, so a folder not chosen adds nothing
    const folder = start ? await agent.folderToUse() : null;
    if (start && !folder) {
      setBusy(false);
      // no folder of the repository is known, and none can be asked for here
      if (!agent.where) setMenu(startMore.current);
      return;
    }
    try {
      const text = body.trim();
      const attachments = files.linked(text);
      const item = await api.addItem({
        repo,
        title: title.trim(),
        ...(text ? { body: text } : {}),
        ...(Object.keys(attachments).length ? { attachments } : {}),
        ...(parent ? { parent } : {}),
      });
      setTitle("");
      setBody("");
      files.reset();
      countFeature("quick_add");
      // the item as the agent's task, with its files
      const started =
        !!folder &&
        (await agent.start(
          await api.item(item.key).catch(() => ({ ...item, body: text, files: [], timeline: [] })),
          folder,
        ));
      onAdded(item, more, started);
    } catch (error) {
      toast(errorMessage(error), "danger");
    } finally {
      setBusy(false);
      field.current?.focus();
    }
  };

  const onKey = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (composing(event)) return;
    if (event.key === "Enter" && event.altKey && agent.chosen) {
      event.preventDefault();
      void add(false, true);
    } else if (event.key === "Enter") {
      event.preventDefault();
      void add(event.metaKey || event.ctrlKey);
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onClose();
    } else if (event.key === "Tab" && event.shiftKey && sorted.length) {
      event.preventDefault();
      setRepo(sorted[(sorted.indexOf(repo) + 1) % sorted.length]);
      setParent("");
    } else if (event.key === "Tab" && !event.shiftKey && !event.altKey && !event.metaKey && !event.ctrlKey) {
      // the description comes next, past the repository and parent controls
      event.preventDefault();
      bodyField.current?.focus();
    }
  };

  // in the description, Enter starts a new line: ⌘Enter adds, ⌘⇧Enter
  // adds and starts another, and ⌘⌥Enter adds and starts the agent
  const onBodyKey = (event: ReactKeyboardEvent<HTMLTextAreaElement>) => {
    if (composing(event)) return;
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey) && event.altKey && agent.chosen) {
      event.preventDefault();
      void add(false, true);
    } else if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      void add(event.shiftKey);
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onClose();
    } else if (event.key === "Tab" && !event.altKey && !event.metaKey && !event.ctrlKey) {
      // Tab moves between the title and the description
      event.preventDefault();
      field.current?.focus();
    }
  };

  const startHint = agent.chosen
    ? [{ keys: inBody ? [MOD, ALT, "Enter"] : [ALT, "Enter"], label: "Add and start" }]
    : [];
  const hints = inBody
    ? [
        { keys: [MOD, "Enter"], label: enterHint },
        { keys: [MOD, SHIFT, "Enter"], label: "Add more" },
        ...startHint,
        { keys: ["Tab"], label: "Title" },
        { keys: ["Esc"], label: "Cancel" },
      ]
    : [
        { keys: ["Enter"], label: enterHint },
        { keys: [MOD, "Enter"], label: "Add more" },
        ...startHint,
        { keys: ["Tab"], label: "Description" },
        { keys: [SHIFT, "Tab"], label: "Repository" },
        { keys: ["Esc"], label: "Cancel" },
      ];
  // without a folder of the repository, it asks for one first
  const startLabel = agent.chosen ? `Add and start ${agent.chosen.name}${agent.where ? "" : "…"}` : "";

  return (
    <div className="item-quick-box" data-testid="item-quick-box">
      <div className="item-quick-line">
        <div className="item-quick-main">
          <ItemStatusIcon status="open" />
          <input
            ref={field}
            className="item-quick-title"
            aria-label="What needs to be done?"
            placeholder="What needs to be done?"
            autoComplete="off"
            autoCorrect="off"
            autoCapitalize="off"
            spellCheck={false}
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            onKeyDown={onKey}
          />
        </div>
        <div className="item-quick-controls">
          <Select
            id="item-quick-repo"
            searchable
            label="Repository"
            icon={<FolderGit2 size={13} />}
            value={repo}
            onChange={(v) => {
              setRepo(v);
              setParent("");
            }}
            options={sorted.map((r) => ({ value: r, label: r }))}
          />
          <Select
            id="item-quick-parent"
            searchable
            label="Parent"
            value={parent}
            onChange={setParent}
            options={[
              { value: "", label: "No parent" },
              ...parents.map((p) => ({ value: p.key, label: `${p.key} ${p.title}` })),
            ]}
          />
          <Tooltip label="Add it" side="bottom">
            <button
              type="button"
              className="item-quick-action is-add"
              aria-label="Add it"
              disabled={!title.trim() || !repo || busy || files.busy}
              onClick={() => void add(false)}
            >
              <Plus size={14} />
            </button>
          </Tooltip>
          <Tooltip label="Cancel" side="bottom">
            <button type="button" className="item-quick-action" aria-label="Cancel" onClick={onClose}>
              <X size={14} />
            </button>
          </Tooltip>
        </div>
      </div>
      <div className="item-quick-body" {...files.handlers}>
        <textarea
          ref={bodyField}
          aria-label="Description"
          placeholder="Add a description… Paste or drop files to attach them, and type # to name an item."
          rows={2}
          value={body}
          onChange={(e) => setBody(e.target.value)}
          onKeyDown={onBodyKey}
          onFocus={() => setInBody(true)}
          onBlur={() => setInBody(false)}
        />
        {picker}
      </div>
      <div className="item-quick-keys">
        <KeyHints hints={hints} />
        {files.busy ? <span className="item-quick-note">Uploading…</span> : null}
        {repos.length === 0 ? (
          <span className="item-quick-note">Name a repository: an item belongs to one.</span>
        ) : null}
        {agent.chosen ? (
          <span className="item-quick-start">
            <Tooltip
              label={
                agent.where
                  ? `${startLabel} in ${baseName(agent.where)}`
                  : `Choose a folder for ${repo}, then add and start`
              }
              side="bottom"
            >
              <button
                type="button"
                aria-label={startLabel}
                disabled={!title.trim() || !repo || busy || files.busy}
                onClick={() => void add(false, true)}
              >
                <Play size={13} aria-hidden="true" />
                <AgentIcon requestedBy={agent.chosen.id} size={13} />
                {startLabel}
              </button>
            </Tooltip>
            <button
              ref={startMore}
              type="button"
              className="item-quick-start-more"
              aria-label="Choose the agent and the folder"
              aria-haspopup="menu"
              aria-expanded={!!menu}
              onClick={(event) => {
                const button = event.currentTarget;
                setMenu((open) => (open ? null : button));
              }}
            >
              <ChevronDown size={13} aria-hidden="true" />
            </button>
            {menu ? (
              <PopoverMenu
                anchor={menu}
                align="right"
                label="Agent and folder"
                entries={agent.entries}
                onClose={() => {
                  setMenu(null);
                  field.current?.focus();
                }}
              />
            ) : null}
          </span>
        ) : null}
      </div>
    </div>
  );
}
