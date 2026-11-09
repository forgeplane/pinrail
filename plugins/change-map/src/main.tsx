// The view connects to the app once, here where the page starts, and draws
// the review each time the app hands it over. React may mount a component
// more than once, and a second connection would look to the app like
// another page in the view's place.
import "@xyflow/react/dist/style.css";
import "./styles.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App, view, type Decision, type Payload } from "./App";
import type { Init } from "../pinrail-plugin";

const { Pinrail } = window;
const root = createRoot(document.getElementById("app")!);
// each init (the first, and another when the review ends while it is open)
// draws the view afresh
let inits = 0;
let shown: Init<Payload, Decision> | null = null;
const draw = (init: Init<Payload, Decision>) => {
  shown = init;
  root.render(
    <StrictMode>
      <App key={inits} plugin={plugin} init={init} />
    </StrictMode>,
  );
};

const plugin = Pinrail.connect<Payload, Decision>({
  onInit: (init) => {
    inits++;
    draw(init);
  },
  // the view's own hand-over was accepted: the same view, now read-only,
  // keeping what is selected and open
  onSubmitted: () => {
    if (shown) draw({ ...shown, readonly: true, review: plugin.review ?? shown.review });
  },
  // the app's hand-over button, or ⌘/Ctrl+Enter: the view's decision
  onCollect: () => view.collect(),
  onSettings: (settings) => view.settings(settings),
});
