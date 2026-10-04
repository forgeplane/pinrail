// The view connects to the app once, here where the page starts, and draws
// the review each time the app hands it over. React may mount a component
// more than once, and a second connection would look to the app like
// another page in the view's place.
import { createRoot } from "react-dom/client";
import { App, view } from "./App";
import "./styles.css";

const root = createRoot(document.getElementById("root")!);
// each init (the first, and another when the review ends while it is open)
// draws the view afresh
let inits = 0;

const plugin = window.Pinrail.connect({
  onInit: (init) => root.render(<App key={++inits} plugin={plugin} init={init} />),
  // the app's hand-over button, or ⌘/Ctrl+Enter: the view's decision
  onCollect: () => view.collect(),
  onViolations: (errors) => view.violations(errors),
  onSubmitted: (decided) => view.submitted(decided),
});
