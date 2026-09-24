// What both versions of the page say, so the review and the classic page
// never drift apart on a fact.
// the plugins' views as `mise run screenshots` makes them, in the site's light theme
import review from "../../assets/screenshots/review-view-light.png";
import email from "../../assets/screenshots/email-view-light.png";
import artifact from "../../assets/screenshots/artifact-view-light.png";
import list from "../../assets/screenshots/list-view-light.png";
import feedback from "../../assets/screenshots/feedback-view-light.png";
// sample plugins made with the SDK, not shipped with the app; their views as
// the experiments' scripts/site-shots.mjs makes them
import image from "../../assets/samples/image-view-light.png";
import audio from "../../assets/samples/audio-view-light.png";
import compare from "../../assets/samples/compare-view-light.png";
import palette from "../../assets/samples/palette-view-light.png";

export const repo = "https://github.com/forgeplane/pinrail";
export const releases = `${repo}/releases`;

export const agents = [
  { name: "Claude Code", icon: "claude" }, { name: "Codex", icon: "codex" }, { name: "Cursor", icon: "cursor" },
  { name: "Gemini CLI", icon: "gemini" }, { name: "OpenCode", icon: "opencode" }, { name: "Kimi", icon: "kimi" },
];

export const views = [
  { name: "Code review", plugin: "review", image: review, detail: "The diff, with the agent's proposed comments on the lines they are about. Accept one with a note, reject the next with a reason.", alt: "Pinrail's code review view: a TypeScript diff with a finding accepted and a revision note being written." },
  { name: "Email drafts", plugin: "email", image: email, detail: "Edit the draft itself and see your changes against it, leave an instruction on a passage, then send, revise or discard each one.", alt: "Pinrail's email view: a draft with an edit showing against the original and a comment on one passage." },
  { name: "Pages & designs", plugin: "artifact", image: artifact, detail: "Click an element on the rendered page. Your comment goes back with the selector the agent needs.", alt: "Pinrail's artifact view: a landing page with comments pinned to elements and one being written." },
  { name: "Lists", plugin: "list", image: list, detail: "Findings, tasks, proposed actions: a verdict and a note on every item, grouped the way the agent sent them.", alt: "Pinrail's list view: dependency upgrades, some accepted, one held back with a reason." },
  { name: "Questions", plugin: "feedback", image: feedback, detail: "When the agent needs your call before it starts: choices with its recommendation, follow-ups, and room to qualify an answer.", alt: "Pinrail's feedback view: questions about an API change, answered, with a comment on one." },
];

export const samples = [
  { name: "Images & illustrations", plugin: "image", image, detail: "Box a region or pin a point and say what to change. Each one goes back in pixels and in fractions of the image, ready for an inpainting pass.", alt: "A sample image plugin: four illustrations in a rail, one on the stage with two regions boxed and a note being written on a third." },
  { name: "Voice & audio", plugin: "audio", image: audio, detail: "The waveform and the transcript in sync. Comment on a word or a stretch, mark what to cut, and hear the take without the cuts.", alt: "A sample audio plugin: four voice takes, one open with its waveform, two comments, a cut and the transcript underneath." },
  { name: "Before & after", plugin: "compare", image: compare, detail: "Did the revision fix what you asked? Wipe, fade or diff the two, and mark each claimed fix fixed, partly or not fixed.", alt: "A sample compare plugin: a pricing card with its changed areas boxed, and one requested fix marked not fixed." },
  { name: "Colour systems", plugin: "palette", image: palette, detail: "Each palette on a sample screen, light and dark, with contrast measured. Click a colour to comment on its token or try a new value.", alt: "A sample palette plugin: a colour system on an app screen in light and dark, beside three other palettes." },
];
