// The view connects to the app once, here where the page starts, and mounts
// the component afresh each time the app hands the review over (the first
// time, and again when the review ends while it is open). A component may
// mount more than once, and a second connection would look to the app like
// another page in the view's place.
import { mount, unmount } from "svelte";
import App, { type Decision, type Payload, type View } from "./App.svelte";

const { Pinrail } = window;
const target = document.getElementById("app")!;
// what the view on screen fills in, for the connection to call
const view: View = { collect: () => undefined, violations: () => {}, submitted: () => {} };
let app: Record<string, unknown> | null = null;

const plugin = Pinrail.connect<Payload, Decision>({
  onInit(init) {
    if (app) void unmount(app);
    app = mount(App, { target, props: { plugin, init, view } });
  },
  // the app's hand-over button, or ⌘/Ctrl+Enter: the view's decision
  onCollect: () => view.collect(),
  onViolations: (errors) => view.violations(errors),
  onSubmitted: () => view.submitted(),
});
