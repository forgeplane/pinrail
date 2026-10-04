// The SDK is a script the app serves; this is the part of it the plugin uses.

export type Review = {
  id: string;
  title: string;
  status: string;
  payload: Record<string, unknown>;
  decision?: { data: Record<string, unknown> } | null;
};

export type Init = {
  review: Review;
  previous: Review | null;
  readonly: boolean;
  draft: Record<string, unknown> | null;
};

export type Violation = { path: string; message: string };

export type Handlers = {
  onInit?: (init: Init) => void;
  onViolations?: (errors: Violation[]) => void;
  onSubmitted?: (decision: { data: Record<string, unknown> } | null) => void;
  onAppearance?: (theme: "dark" | "light") => void;
  /** the app's hand-over: the decision, or nothing to hand over yet */
  onCollect?: () => unknown;
};

export type Plugin = {
  readonly readonly: boolean;
  readonly theme: "dark" | "light";
  draft: (data: unknown) => void;
  handOverLabel: (text: string) => void;
  /** a file the review carries, by the name its payload gives it */
  attachment: (name: string) => Promise<ArrayBuffer>;
};

declare global {
  interface Window {
    Pinrail: {
      connect: (handlers: Handlers) => Plugin;
      escape: (s: string) => string;
      markdown: (s: string) => string;
      attachmentName: (ref: unknown) => string | null;
    };
  }
}
