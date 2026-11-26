// Renders a component for a test in the browser, as the app would: in a
// router at a given address, inside the app's providers. The test file
// replaces the API client with the fake core first (see core.ts), since the
// providers load from it.

import { act, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { MemoryRouter } from "react-router";
import { LiveProvider } from "../state/live";
import { SettingsProvider } from "../state/settings";
import { ToastProvider } from "../state/toasts";
import { TopBarProvider } from "../state/topbar";

// React checks that its updates are wrapped in act() only while the flag
// is on: on for rendering and unmounting, which act() completes, and off
// for the updates the app makes on its own in the browser, such as after a
// fetch, which the tests wait for as a person would
const flag = globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean };

async function inAct(work: () => void) {
  flag.IS_REACT_ACT_ENVIRONMENT = true;
  try {
    await act(async () => work());
  } finally {
    flag.IS_REACT_ACT_ENVIRONMENT = false;
  }
}

const mounted = new Set<() => void>();

/** Renders `ui` at `route` with the app's providers; `bare` leaves the
 *  providers out, for a component that needs none. */
export async function render(ui: ReactNode, { route = "/", bare = false } = {}) {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  const tree = bare ? (
    ui
  ) : (
    <LiveProvider>
      <TopBarProvider>
        <SettingsProvider>
          <ToastProvider>{ui}</ToastProvider>
        </SettingsProvider>
      </TopBarProvider>
    </LiveProvider>
  );
  await inAct(() => root.render(<MemoryRouter initialEntries={[route]}>{tree}</MemoryRouter>));
  const unmount = () => {
    flag.IS_REACT_ACT_ENVIRONMENT = true;
    act(() => root.unmount());
    flag.IS_REACT_ACT_ENVIRONMENT = false;
    container.remove();
    mounted.delete(unmount);
  };
  mounted.add(unmount);
  return { container, unmount };
}

/** Unmounts what the test rendered; call it in `afterEach`. */
export function cleanup() {
  for (const unmount of [...mounted]) unmount();
}
