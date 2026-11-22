// The button in a work item's panel that starts an agent on the item at
// once: the agent used last, in the folder of the item's repository used
// last, with the item's description, comments and files as its task. The
// new session claims the item. The arrow beside it changes the agent and
// the folder.

import { ChevronDown, Folder, FolderOpen, Play } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { inTauri } from "../api/client";
import { runtime, type RecentFolder, type StartableAgent } from "../api/runtime";
import type { ItemDetail } from "../api/types";
import { useToast } from "../state/toasts";
import { AgentIcon } from "./AgentIcon";
import { baseName, refusal } from "./NewSession";
import { PopoverMenu, type MenuEntry } from "./PopoverMenu";
import { sessionGrid } from "./session/startRun";
import { Tooltip } from "./Tooltip";
import { itemTask } from "../lib/itemTask";

export function ItemQuickStart({ item }: { item: ItemDetail }) {
  const toast = useToast();
  const [agents, setAgents] = useState<StartableAgent[]>([]);
  const [folders, setFolders] = useState<RecentFolder[]>([]);
  const [agent, setAgent] = useState<string | null>(null);
  const [folder, setFolder] = useState<string | null>(null);
  const [menu, setMenu] = useState<HTMLElement | null>(null);
  const [starting, setStarting] = useState(false);
  const more = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    let current = true;
    runtime.startable().then(
      (found) => {
        if (!current) return;
        setAgents(found);
        setAgent(
          (chosen) =>
            chosen ?? (found.find((a) => a.last_used && a.installed) ?? found.find((a) => a.installed))?.id ?? null,
        );
      },
      () => {},
    );
    runtime.recentFolders().then(
      (found) => current && setFolders(found),
      () => {},
    );
    return () => {
      current = false;
    };
  }, []);

  const installed = agents.filter((a) => a.installed);
  const chosen = installed.find((a) => a.id === agent);
  // the folders of the item's repository, the one used last first
  const ours = folders.filter((f) => f.exists && f.repo === item.repo);
  const where = folder ?? ours[0]?.folder ?? null;
  if (!chosen) return null;

  const chooseFolder = async (): Promise<string | null> => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const picked = await open({ directory: true, multiple: false, title: "Choose a folder" });
    return typeof picked === "string" ? picked : null;
  };

  const start = async () => {
    if (starting) return;
    // no folder of the repository is known: ask for one
    if (!where && !inTauri()) return setMenu(more.current);
    const folderToUse = where ?? (await chooseFolder());
    if (!folderToUse) return;
    if (!where) setFolder(folderToUse);
    setStarting(true);
    try {
      const answer = await runtime.start({
        agent: chosen.id,
        folder: folderToUse,
        task: itemTask(item),
        item: item.key,
        local_id: crypto.randomUUID(),
        ...(await sessionGrid()),
      });
      if (answer.claim_refused)
        toast(`${chosen.name} started, but it could not claim ${item.key}: ${answer.claim_refused}`, "danger");
      else toast(`Started ${chosen.name} on ${item.key}`);
    } catch (e) {
      toast(refusal(e, chosen), "danger");
    } finally {
      setStarting(false);
    }
  };

  const entries: MenuEntry[] = [
    { kind: "heading", label: "Agent" },
    ...installed.map((a): MenuEntry => ({
      label: a.name,
      mark: <AgentIcon requestedBy={a.id} size={14} />,
      hint: a.version ?? undefined,
      checked: a.id === chosen.id,
      onPick: () => setAgent(a.id),
    })),
    { kind: "separator" },
    { kind: "heading", label: "Folder" },
    ...ours.map((f): MenuEntry => ({
      label: baseName(f.folder),
      icon: Folder,
      hint: f.branch ?? undefined,
      checked: f.folder === where,
      onPick: () => setFolder(f.folder),
    })),
    // a folder chosen outside the list stays offered
    ...(where && !ours.some((f) => f.folder === where)
      ? [{ label: baseName(where), icon: Folder, checked: true, onPick: () => {} } as MenuEntry]
      : []),
    ...(inTauri()
      ? [
          {
            label: "Choose a folder…",
            icon: FolderOpen,
            onPick: () => {
              void chooseFolder().then((picked) => picked && setFolder(picked));
            },
          } as MenuEntry,
        ]
      : []),
  ];

  const label = `Start ${chosen.name}${where ? "" : "…"}`;
  return (
    <span className="item-quick-start">
      <Tooltip
        label={where ? `Start ${chosen.name} on ${item.key} in ${baseName(where)}` : `Choose a folder for ${item.key}`}
        side="bottom"
      >
        <button type="button" aria-label={label} disabled={starting} onClick={() => void start()}>
          <Play size={13} aria-hidden="true" />
          <AgentIcon requestedBy={chosen.id} size={13} />
          {label}
        </button>
      </Tooltip>
      <button
        ref={more}
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
        <PopoverMenu anchor={menu} label="Agent and folder" entries={entries} onClose={() => setMenu(null)} />
      ) : null}
    </span>
  );
}
