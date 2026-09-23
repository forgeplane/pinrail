// The SDK is a script the app serves; this is the part of it the plugin uses.

export type Gate = {
  id: string;
  title: string;
  status: string;
  payload: Record<string, unknown>;
  decision?: { data: Record<string, unknown> } | null;
};

export type Init = {
  gate: Gate;
  previous: Gate | null;
  readonly: boolean;
  draft: Record<string, unknown> | null;
};

export type Violation = { path: string; message: string };

export type Handlers = {
  resize?: "auto" | "fill" | "manual";
  onInit?: (init: Init) => void;
  onViolations?: (errors: Violation[]) => void;
  onSubmitted?: (decision: { data: Record<string, unknown> } | null) => void;
  onAppearance?: (theme: "dark" | "light") => void;
  onCollect?: () => void;
};

export type Plugin = {
  readonly readonly: boolean;
  readonly theme: "dark" | "light";
  submit: (data: unknown) => void;
  draft: (data: unknown, opts?: { flush?: boolean }) => void;
  status: (status: { label: string }) => void;
  collect: () => void;
};

declare global {
  interface Window {
    Pinrail: {
      connect: (handlers: Handlers) => Plugin;
      escape: (s: string) => string;
      markdown: (s: string) => string;
    };
  }
}
