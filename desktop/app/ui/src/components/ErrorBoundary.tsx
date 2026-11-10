import { Component, type ErrorInfo, type ReactNode } from "react";
import { reportCrash } from "../lib/crashes";
import { TriangleAlert } from "lucide-react";
import { EmptyState } from "./EmptyState";

type Props = { resetKey: string; children: ReactNode };
type State = { error: Error | null; key: string };

/** Shows what failed in place of a screen that throws while it renders, so
 *  the sidebar and the other screens stay usable. When `resetKey` changes,
 *  as it does when the person goes to another page, it renders the screens
 *  again. */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null, key: this.props.resetKey };

  static getDerivedStateFromError(error: unknown): Partial<State> {
    return { error: error instanceof Error ? error : new Error(String(error)) };
  }

  static getDerivedStateFromProps(props: Props, state: State): Partial<State> | null {
    return props.resetKey === state.key ? null : { error: null, key: props.resetKey };
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    console.error("A screen failed to render", error, info.componentStack);
    reportCrash(error);
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <EmptyState
        title="This page could not be shown"
        icon={<TriangleAlert size={28} strokeWidth={1.5} />}
        actions={
          <button type="button" className="chrome-button" onClick={() => window.location.reload()}>
            Reload
          </button>
        }
      >
        {error.message || "An unexpected error occurred."}
      </EmptyState>
    );
  }
}
