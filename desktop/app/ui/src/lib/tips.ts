// The tips the New session page shows, one at a time. Each says something
// that works today. A shortcut is written in braces, its keys joined by
// "+", such as {MOD+N}: the page draws each key as a key cap, named as the
// platform names it.

import { ALT, isMac, MOD, SHIFT, TRAY } from "./keys";

export type Tip = { category: string; text: string };

const ALL: (Tip & { macOnly?: boolean })[] = [
  {
    category: "Starting a session",
    text: "Leave the message empty to start the agent with nothing to do, then talk to it in its terminal.",
  },
  { category: "Starting a session", text: "Type # in the message to point the agent at a work item." },
  { category: "Starting a session", text: "{MOD+N} opens this page from anywhere in Pinrail." },
  { category: "Starting a session", text: "Arguments passes flags to the agent, such as --model." },
  {
    category: "Starting a session",
    text: "Type a path in the folder picker to start in a folder you have not used before.",
  },
  { category: "Starting a session", text: "The agent you use most and the folder you used last are picked for you." },
  { category: "Sessions", text: "Within a session, {Ctrl+`} switches between its summary and its terminal." },
  { category: "Sessions", text: "{MOD+SHIFT+X} stops the agent of the session you are looking at." },
  { category: "Sessions", text: "{MOD+F} finds text in a session's thread." },
  {
    category: "Sessions",
    text: "From a session's ⋯ menu, or a right-click on it in the sidebar, you can archive it, stop its agent, mark it unread or copy its id.",
  },
  { category: "Sessions", text: "{E} archives the session you are looking at on the Sessions page." },
  { category: "Sessions", text: "A session whose agent stopped can be resumed from its header." },
  {
    category: "Sessions",
    text: "A session that runs in another terminal can be opened in Pinrail, for agents that support it.",
  },
  {
    category: "Sessions",
    text: "A message to a session Pinrail runs is typed into its terminal; otherwise the agent gets it with its next status.",
  },
  { category: "Sessions", text: "A session that waits for your answer shows Needs you in the sidebar." },
  {
    category: "Sessions",
    text: "Group the sidebar's sessions by project or by repository from the menu on its heading.",
  },
  {
    category: "Questions",
    text: "{1}–{9} pick a choice of the current question, and {N} and {P} move between questions.",
  },
  { category: "Questions", text: "{C} adds a note to your answer." },
  { category: "Questions", text: "{MOD+Enter} sends your answers, and {MOD+D} declines the questions." },
  { category: "Questions", text: "When the agent suggests a reply, {Tab} takes it." },
  { category: "Reviews", text: "{J} and {K} move through the inbox, and {Enter} opens the focused review." },
  { category: "Reviews", text: "{MOD+Enter} hands a review back to the agent." },
  { category: "Reviews", text: "{MOD+SHIFT+S} stars a review, and a session's details list its starred reviews." },
  { category: "Reviews", text: "{[} and {]} move between the rounds of a review." },
  { category: "Reviews", text: "{MOD+SHIFT+M} maximizes a review's view." },
  {
    category: "Reviews",
    text: "{ALT+SHIFT+W} brings Pinrail forward from any app with the oldest waiting review. Change it in Settings › Shortcuts.",
  },
  { category: "Reviews", text: `The icon in ${TRAY} counts the reviews that wait on you.` },
  { category: "Work items", text: "{MOD+SHIFT+C} adds a work item from any page." },
  {
    category: "Work items",
    text: "On the Work items page, {S} claims an item, {D} marks it done, {H} puts it on hold and {Q} queues it.",
  },
  { category: "Work items", text: "An item on hold is never taken by an agent: pinrail items next passes over it." },
  { category: "Work items", text: "An agent cannot claim a work item while an item it depends on is open." },
  { category: "Work items", text: "pinrail items next --claim gives an agent the next ready item." },
  { category: "Getting around", text: "{MOD+K} searches everything: reviews, sessions, work items and commands." },
  { category: "Getting around", text: "{/} finds text on the page you are on." },
  { category: "Getting around", text: "{MOD+1} to {MOD+4} open the Inbox, Sessions, Work items and Projects." },
  { category: "Getting around", text: "{MOD+B} shows or hides the sidebar." },
  { category: "Getting around", text: "{?} lists every keyboard shortcut." },
  { category: "Getting around", text: "{MOD+SHIFT+L} switches between the light and dark themes." },
  {
    category: "Settings",
    text: "Connect an agent in Settings › Agents, and its sessions appear in the sidebar on their own.",
  },
  { category: "Settings", text: "Update an agent in Settings › Agents when Pinrail says its skill or hooks changed." },
  {
    category: "Settings",
    text: "Settings › Terminal chooses the terminal's renderer, and whether Option acts as Meta.",
    macOnly: true,
  },
  { category: "Settings", text: "Settings › Terminal chooses the terminal's renderer.", macOnly: false },
  {
    category: "Settings",
    text: "You can add your own theme as a file. Settings › Appearance lists it with the built-in ones.",
  },
  { category: "Settings", text: "Plugins add new kinds of review. Settings › Plugins installs and removes them." },
];

/** The tips for this platform. */
export const TIPS: Tip[] = ALL.filter((t) => t.macOnly === undefined || t.macOnly === isMac).map(
  ({ category, text }) => ({ category, text }),
);

/** The names of the keys a shortcut's word stands for. */
const NAMES: Record<string, string> = { MOD, SHIFT, ALT, Enter: isMac ? "↵" : "Enter" };

/** A tip's text, split into plain text and shortcuts, each a list of keys. */
export function tipParts(text: string): (string | string[])[] {
  return text
    .split(/(\{[^{}]+\})/)
    .filter(Boolean)
    .map((part) =>
      /^\{[^{}]+\}$/.test(part)
        ? part
            .slice(1, -1)
            .split(/\+(?=.)/)
            .map((key) => NAMES[key] ?? key)
        : part,
    );
}
