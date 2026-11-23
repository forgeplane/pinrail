// The button in a work item's panel that starts an agent on the item at
// once: the agent used most, in the folder of the item's repository used
// last, with the item's description, comments and files as its task. The
// new session claims the item. The arrow beside it changes the agent and
// the folder.

import { ChevronDown, Play } from "lucide-react";
import { useRef, useState } from "react";
import { inTauri } from "../api/client";
import type { ItemDetail } from "../api/types";
import { AgentIcon } from "./AgentIcon";
import { baseName } from "./NewSession";
import { PopoverMenu } from "./PopoverMenu";
import { Tooltip } from "./Tooltip";
import { useAgentStart } from "./useAgentStart";

export function ItemQuickStart({ item }: { item: ItemDetail }) {
  const { chosen, where, entries, folderToUse, start: startOn } = useAgentStart(item.repo);
  const [menu, setMenu] = useState<HTMLElement | null>(null);
  const [starting, setStarting] = useState(false);
  const more = useRef<HTMLButtonElement>(null);
  if (!chosen) return null;

  const start = async () => {
    if (starting) return;
    const folder = await folderToUse();
    if (!folder) {
      // no folder of the repository is known, and none can be asked for here
      if (!inTauri()) setMenu(more.current);
      return;
    }
    setStarting(true);
    try {
      await startOn(item, folder);
    } finally {
      setStarting(false);
    }
  };

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
