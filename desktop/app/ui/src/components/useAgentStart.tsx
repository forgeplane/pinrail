// Starting an agent on a work item at once: the agent used last, in the
// folder of the item's repository used last, with the item's description,
// comments and files as its task. The new session claims the item. The
// item page's Start button and the quick add both use it, with a menu that
// changes the agent and the folder.

import { Folder, FolderOpen } from "lucide-react";
import { useEffect, useState } from "react";
import { inTauri } from "../api/client";
import { runtime, type RecentFolder, type StartableAgent } from "../api/runtime";
import { itemTask, type TaskItem } from "../lib/itemTask";
import { useToast } from "../state/toasts";
import { AgentIcon } from "./AgentIcon";
import { baseName, refusal } from "./NewSession";
import type { MenuEntry } from "./PopoverMenu";
import { sessionGrid } from "./session/startRun";

/** Asks for a folder in the system's dialog, in the app. */
const chooseFolder = async (): Promise<string | null> => {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ directory: true, multiple: false, title: "Choose a folder" });
  return typeof picked === "string" ? picked : null;
};

/** The agent and the folder to start on an item of `repo`, while `enabled`. */
export function useAgentStart(repo: string, enabled = true) {
  const toast = useToast();
  const [agents, setAgents] = useState<StartableAgent[]>([]);
  const [folders, setFolders] = useState<RecentFolder[]>([]);
  const [agent, setAgent] = useState<string | null>(null);
  const [picked, setPicked] = useState<{ repo: string; folder: string } | null>(null);

  useEffect(() => {
    if (!enabled) return;
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
  }, [enabled]);

  const installed = agents.filter((a) => a.installed);
  const chosen = enabled ? installed.find((a) => a.id === agent) : undefined;
  // the folders of the item's repository, the one used last first
  const ours = folders.filter((f) => f.exists && f.repo === repo);
  // a folder chosen for another repository does not carry over
  const where = (picked?.repo === repo ? picked.folder : null) ?? ours[0]?.folder ?? null;
  const setFolder = (folder: string) => setPicked({ repo, folder });

  /** The folder to start in: the one known, else one asked for in the
   *  app. Null when none is chosen, or none can be asked for here. */
  const folderToUse = async (): Promise<string | null> => {
    if (where) return where;
    if (!inTauri()) return null;
    const folder = await chooseFolder();
    if (folder) setFolder(folder);
    return folder;
  };

  /** Starts the agent on `item` in `folder`, says how it went, and
   *  answers whether it started. */
  const start = async (item: TaskItem, folder: string): Promise<boolean> => {
    if (!chosen) return false;
    try {
      const answer = await runtime.start({
        agent: chosen.id,
        folder,
        task: itemTask(item),
        item: item.key,
        local_id: crypto.randomUUID(),
        ...(await sessionGrid()),
      });
      if (answer.claim_refused)
        toast(`${chosen.name} started, but it could not claim ${item.key}: ${answer.claim_refused}`, "danger");
      else toast(`Started ${chosen.name} on ${item.key}`);
      return true;
    } catch (e) {
      toast(refusal(e, chosen), "danger");
      return false;
    }
  };

  const entries: MenuEntry[] = chosen
    ? [
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
                  void chooseFolder().then((folder) => folder && setFolder(folder));
                },
              } as MenuEntry,
            ]
          : []),
      ]
    : [];

  return { chosen, where, entries, folderToUse, start };
}
