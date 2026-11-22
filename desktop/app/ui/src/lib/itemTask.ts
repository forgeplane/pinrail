// The first prompt of an agent started on a work item: the item's key,
// title, description and comments, as Markdown. An agent cannot open the
// attachment:NAME links the app shows files by, so each link names the
// file instead, and the prompt ends with the command that saves the files.

import type { ItemDetail } from "../api/types";

export type TaskItem = Pick<ItemDetail, "key" | "title" | "body" | "files" | "timeline">;

// an image or a link to one of the item's files, as the app writes it
const ATTACHMENT_LINK = /!?\[[^\]]*\]\(attachment:([^)\s]+)\)/g;

const decoded = (name: string) => {
  try {
    return decodeURIComponent(name);
  } catch {
    return name;
  }
};

const named = (text: string) =>
  text.replace(ATTACHMENT_LINK, (_link, name: string) => `attached file \`${decoded(name)}\``).trim();

export const itemTask = (item: TaskItem) => {
  const parts = [`# ${item.key} ${item.title}`];
  const body = named(item.body);
  if (body) parts.push(body);

  const comments = item.timeline.flatMap((e) => (e.type === "comment" ? [e] : []));
  if (comments.length) {
    parts.push("## Comments");
    for (const c of comments) parts.push(`**${c.author}**, ${c.created_at.slice(0, 10)}:\n\n${named(c.body)}`);
  }

  const files = [...new Set([...item.files, ...comments.flatMap((c) => c.files)].map((f) => f.name))];
  if (files.length) {
    const list = files.map((name) => `\`${name}\``).join(", ");
    const them = files.length === 1 ? "it and print its path" : "them and print their paths";
    parts.push("## Attached files", `${list}. Run \`pinrail items show ${item.key}\` to save ${them}.`);
  }
  return parts.join("\n\n");
};
