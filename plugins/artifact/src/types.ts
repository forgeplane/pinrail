export type Kind = "change" | "question" | "praise";
export type Verdict = "approve" | "revise";

export type Comment = {
  id: string;
  selector: string;
  tag: string;
  kind: Kind;
  text: string;
  snippet?: string;
  html?: string;
};

export type Decision = { verdict: Verdict; comments: Comment[] };

export type Payload = {
  title?: string;
  /** the document inline, or `file`, the same sent beside the payload */
  html?: string;
  file?: { $attachment: string };
  notes?: string;
  viewport?: Viewport;
};

export type Viewport = "desktop" | "tablet" | "phone";

export const VIEWPORTS: { key: Viewport; label: string; width: number | null }[] = [
  { key: "desktop", label: "Desktop", width: null },
  { key: "tablet", label: "Tablet", width: 820 },
  { key: "phone", label: "Phone", width: 390 },
];

export const KINDS: { key: Kind; label: string }[] = [
  { key: "change", label: "Change" },
  { key: "question", label: "Question" },
  { key: "praise", label: "Keep" },
];

export const newId = () => `c_${Math.random().toString(36).slice(2, 8)}${Date.now().toString(36).slice(-4)}`;
