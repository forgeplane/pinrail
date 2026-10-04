// The view connects to the app once, here where the page starts, and draws
// the review each time the app hands it over. React may mount a component
// more than once, and a second connection would look to the app like
// another page in the view's place.
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App, view, type Decision, type Payload } from "./App";

const { Pinrail } = window;
const root = createRoot(document.getElementById("app")!);
// each init (the first, and another when the review ends while it is open)
// draws the view afresh
let inits = 0;

const plugin = Pinrail.connect<Payload, Decision>({
  onInit: (init) =>
    root.render(
      <StrictMode>
        <App key={++inits} plugin={plugin} init={init} />
      </StrictMode>,
    ),
  // the app's hand-over button, or ⌘/Ctrl+Enter: the view's decision
  onCollect: () => view.collect(),
  onViolations: (errors) => view.violations(errors),
  onSubmitted: () => view.submitted(),
});
