// The videos the docs show, by the name a page gives after `video:`. The
// files are too large for the repository: each video and its poster are
// served from media.pinrail.dev. A file there is cached for a year, so a new
// cut gets a new name.

export const videos = {
  // the two-minute demo; its chapters are the times of its scenes, each with
  // a line on what it shows
  demo: {
    src: "https://media.pinrail.dev/demo-v1.mp4",
    poster: "https://media.pinrail.dev/demo-v1-poster.jpg",
    seconds: 118.5,
    label: "Watch the demo",
    chapters: [
      { at: 0, title: "An agent asks", detail: "It runs one command and waits for your decision." },
      { at: 26.56, title: "A code review", detail: "A diff, with the agent's findings on its lines." },
      { at: 49.87, title: "An image", detail: "Box the part to change and say what you want." },
      { at: 63.07, title: "A new round", detail: "The revision lands beside your earlier comments." },
      { at: 74.77, title: "A plugin of your own", detail: "Ask your agent to build a view for your work." },
    ],
  },
};
