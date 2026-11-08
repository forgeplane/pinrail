// Runs the app's commands from the keys and from the native menu, as
// lib/commands.ts defines them and the layout says what each one does.

import { useEffect, useRef } from "react";
import { COMMANDS, type CommandId } from "../lib/commands";

/** What a command does; `false` when it does not apply now, which leaves
 *  the key to whatever it does elsewhere. */
export type CommandRun = () => void | false;

export function useAppCommands(runs: Partial<Record<CommandId, CommandRun>>) {
  // the latest actions, so the listeners are added once
  const current = useRef(runs);
  current.current = runs;
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      for (const command of COMMANDS) {
        const run = current.current[command.id];
        if (!run || !command.matches?.(event)) continue;
        if (run() !== false) event.preventDefault();
        return;
      }
    };
    const onCommand = (event: Event) => {
      const id = (event as CustomEvent<string>).detail;
      if (COMMANDS.some((c) => c.menu && c.id === id)) current.current[id as CommandId]?.();
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("pinrail:command", onCommand);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pinrail:command", onCommand);
    };
  }, []);
}
