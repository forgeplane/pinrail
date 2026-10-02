// The docs' screenshots. The whole app, shrunk into the text column, is too
// small to read, so a click opens a picture full size. A figure of several
// shows one at a time with a dot for each; zoomed, arrows move between them.

const slidesOf = (figure) => (figure ? [...figure.querySelectorAll(".pr-slide")] : []);

/* the figure's picture `n`: its slide shown, its dot current */
function show(figure, n) {
  slidesOf(figure).forEach((slide, i) => {
    slide.hidden = i !== n;
  });
  figure.querySelectorAll(".pr-dot").forEach((dot, i) => dot.setAttribute("aria-current", String(i === n)));
}

// the picture in the reader's theme, of the ones a button holds
const shown = (button) =>
  [...button.querySelectorAll("img")].find((img) => getComputedStyle(img).display !== "none") ||
  button.querySelector("img");

function openZoom(button) {
  const figure = button.closest(".pr-shots");
  const slides = slidesOf(figure);
  let at = figure ? slides.indexOf(button.closest(".pr-slide")) : 0;

  const dialog = document.createElement("dialog");
  dialog.className = "pr-zoom";
  const img = document.createElement("img");
  const caption = document.createElement("p");
  caption.className = "pr-zoom-caption";
  const counter = document.createElement("span");
  counter.className = "pr-zoom-count";
  const control = (cls, label, text) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = cls;
    b.setAttribute("aria-label", label);
    b.textContent = text;
    return b;
  };
  const close = control("pr-zoom-close", "Close", "×");
  const prev = control("pr-zoom-nav pr-zoom-prev", "Previous picture", "‹");
  const next = control("pr-zoom-nav pr-zoom-next", "Next picture", "›");

  const paint = () => {
    const source = figure ? slides[at].querySelector(".pr-shot-open") : button;
    const pic = shown(source);
    img.src = pic.currentSrc || pic.src;
    img.alt = pic.alt || source.querySelector("img").alt || "";
    dialog.setAttribute("aria-label", img.alt || "Screenshot");
    const title = figure
      ? slides[at].querySelector("figcaption")
      : button.closest("figure")?.querySelector("figcaption");
    caption.textContent = title ? title.textContent : "";
    caption.hidden = !title;
    counter.textContent = figure ? `${at + 1} / ${slides.length}` : "";
    if (figure) show(figure, at);
  };
  const go = (by) => {
    if (!figure) return;
    at = (at + by + slides.length) % slides.length;
    paint();
  };

  dialog.append(close, img, caption);
  if (figure && slides.length > 1) dialog.append(prev, next, counter);
  dialog.addEventListener("click", (event) => {
    if (event.target === prev) {
      event.stopPropagation();
      go(-1);
      return;
    }
    if (event.target === next) {
      event.stopPropagation();
      go(1);
      return;
    }
    dialog.close();
  });
  dialog.addEventListener("keydown", (event) => {
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      go(-1);
    }
    if (event.key === "ArrowRight") {
      event.preventDefault();
      go(1);
    }
  });
  dialog.addEventListener("close", () => {
    dialog.remove();
    (figure ? slides[at].querySelector(".pr-shot-open") : button).focus();
  });
  paint();
  document.body.append(dialog);
  dialog.showModal();
  close.focus();
}

/* A diagram, full size: the one drawn for the reader's theme, on its panel.
   It keeps the figure's class, so its styles (the accent for "you") hold. */
function openDiagram(canvas) {
  const figure = canvas.closest(".pr-mermaid");
  const svg = [...canvas.querySelectorAll("svg")].find((s) => s.getBoundingClientRect().width > 0);
  if (!svg) return;
  const dialog = document.createElement("dialog");
  dialog.className = "pr-zoom";
  dialog.setAttribute("aria-label", figure.getAttribute("aria-label") || "Diagram");
  const panel = document.createElement("div");
  panel.className = "pr-mermaid pr-zoom-diagram";
  panel.append(svg.cloneNode(true));
  const close = document.createElement("button");
  close.type = "button";
  close.className = "pr-zoom-close";
  close.setAttribute("aria-label", "Close");
  close.textContent = "×";
  const title = figure.querySelector("figcaption");
  const caption = document.createElement("p");
  caption.className = "pr-zoom-caption";
  caption.textContent = title ? title.textContent : "";
  caption.hidden = !title;
  dialog.append(close, panel, caption);
  dialog.addEventListener("click", () => dialog.close());
  dialog.addEventListener("close", () => {
    dialog.remove();
    canvas.focus();
  });
  document.body.append(dialog);
  dialog.showModal();
  close.focus();
}

document.addEventListener("keydown", (event) => {
  const canvas = event.target.closest && event.target.closest(".pr-mermaid-canvas");
  if (canvas && (event.key === "Enter" || event.key === " ")) {
    event.preventDefault();
    openDiagram(canvas);
  }
});

document.addEventListener("click", (event) => {
  const canvas = event.target.closest(".pr-mermaid-canvas");
  if (canvas && !canvas.closest(".pr-zoom")) {
    openDiagram(canvas);
    return;
  }
  const dot = event.target.closest(".pr-dot");
  if (dot) {
    show(dot.closest(".pr-shots"), Number(dot.dataset.dot));
    return;
  }
  const button = event.target.closest(".pr-shot-open");
  if (button) openZoom(button);
});

// the arrow keys move between the dots of a figure
document.addEventListener("keydown", (event) => {
  const dot = event.target.closest && event.target.closest(".pr-dot");
  if (!dot || !["ArrowLeft", "ArrowRight"].includes(event.key)) return;
  const dots = [...dot.parentElement.querySelectorAll(".pr-dot")];
  const n = (dots.indexOf(dot) + (event.key === "ArrowRight" ? 1 : -1) + dots.length) % dots.length;
  event.preventDefault();
  show(dot.closest(".pr-shots"), n);
  dots[n].focus();
});

// A plugin's contract: tabs for the manifest, the payload and the decision,
// each as fields or as JSON. The tab and the view are remembered, so every
// plugin page opens on the one the reader last looked at.
const CONTRACT_KEY = "pr-contract";
const remembered = () => {
  try {
    return JSON.parse(localStorage.getItem(CONTRACT_KEY) || "{}");
  } catch {
    return {};
  }
};
function remember(patch) {
  try {
    localStorage.setItem(CONTRACT_KEY, JSON.stringify({ ...remembered(), ...patch }));
  } catch {
    /* storage off: nothing to keep */
  }
}

function showTab(figure, key, focus) {
  figure.querySelectorAll("[data-contract-tab]").forEach((tab) => {
    const on = tab.dataset.contractTab === key;
    tab.setAttribute("aria-selected", String(on));
    tab.tabIndex = on ? 0 : -1;
    if (on && focus) tab.focus();
  });
  figure.querySelectorAll("[data-contract-panel]").forEach((panel) => {
    panel.hidden = panel.dataset.contractPanel !== key;
  });
}

function showView(figure, view) {
  figure
    .querySelectorAll("[data-contract-show]")
    .forEach((b) => b.setAttribute("aria-pressed", String(b.dataset.contractShow === view)));
  figure.querySelectorAll("[data-contract-view]").forEach((v) => {
    v.hidden = v.dataset.contractView !== view;
  });
}

for (const figure of document.querySelectorAll("[data-contract]")) {
  const { tab, view } = remembered();
  if (tab && figure.querySelector(`[data-contract-tab="${tab}"]`)) showTab(figure, tab, false);
  if (view) showView(figure, view);
}

document.addEventListener("click", (event) => {
  const tab = event.target.closest && event.target.closest("[data-contract-tab]");
  if (tab) {
    showTab(tab.closest("[data-contract]"), tab.dataset.contractTab, false);
    remember({ tab: tab.dataset.contractTab });
    return;
  }
  const view = event.target.closest && event.target.closest("[data-contract-show]");
  if (view) {
    showView(view.closest("[data-contract]"), view.dataset.contractShow);
    remember({ view: view.dataset.contractShow });
  }
});

// the arrow keys move between the tabs, as a tab list does
document.addEventListener("keydown", (event) => {
  const tab = event.target.closest && event.target.closest("[data-contract-tab]");
  if (!tab || !["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
  const tabs = [...tab.parentElement.querySelectorAll("[data-contract-tab]")];
  const at = tabs.indexOf(tab);
  const n =
    event.key === "Home"
      ? 0
      : event.key === "End"
        ? tabs.length - 1
        : (at + (event.key === "ArrowRight" ? 1 : -1) + tabs.length) % tabs.length;
  event.preventDefault();
  showTab(tab.closest("[data-contract]"), tabs[n].dataset.contractTab, true);
  remember({ tab: tabs[n].dataset.contractTab });
});

// Code in several frameworks: one choice for the whole page. Picking a tab
// in any group picks it in all of them, and the next page opens on it.
const FRAMEWORK_KEY = "pr-framework";
function pickFramework(key) {
  for (const group of document.querySelectorAll("[data-frameworks]")) {
    if (!group.querySelector(`[data-framework-panel="${key}"]`)) continue;
    group.querySelectorAll("[data-framework-tab]").forEach((tab) => {
      const on = tab.dataset.frameworkTab === key;
      tab.setAttribute("aria-selected", String(on));
      tab.tabIndex = on ? 0 : -1;
    });
    group.querySelectorAll("[data-framework-panel]").forEach((panel) => {
      panel.hidden = panel.dataset.frameworkPanel !== key;
    });
  }
}
try {
  const kept = localStorage.getItem(FRAMEWORK_KEY);
  if (kept) pickFramework(kept);
} catch {
  /* storage off: the first tab */
}
document.addEventListener("click", (event) => {
  const tab = event.target.closest && event.target.closest("[data-framework-tab]");
  if (!tab) return;
  // keep the group that was clicked where it is on screen while the others change height
  const before = tab.getBoundingClientRect().top;
  pickFramework(tab.dataset.frameworkTab);
  window.scrollBy(0, tab.getBoundingClientRect().top - before);
  try {
    localStorage.setItem(FRAMEWORK_KEY, tab.dataset.frameworkTab);
  } catch {
    /* nothing to keep */
  }
});

// The docs' videos. In the page a video is its poster with a play button.
// Played, it leaves its place and opens large over the page, with its own
// bar: what has played, a mark for each chapter, and the chapter's name. The
// chapters listed under the poster open it at their moments.
const clock = (s) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
const calm = matchMedia("(prefers-reduced-motion: reduce)").matches;
// the one video that is open, if any
let open = null;

function setUpVideo(root) {
  const video = root.querySelector("video");
  const dialog = root.querySelector(".pr-video-dialog");
  const shell = root.querySelector(".pr-video-shell");
  const backdrop = root.querySelector(".pr-video-backdrop");
  const frame = root.querySelector(".pr-video-frame");
  const track = root.querySelector(".pr-video-track");
  const progress = root.querySelector(".pr-video-progress");
  const time = root.querySelector(".pr-video-time");
  const chapter = root.querySelector(".pr-video-chapter");
  const toggle = root.querySelector(".pr-video-toggle");
  const mute = root.querySelector(".pr-video-mute");
  const start = root.querySelector(".pr-video-start");
  const chapters = [...root.querySelectorAll(".pr-video-mark")].map((mark) => ({
    at: Number(mark.dataset.at),
    title: mark.querySelector(".pr-video-tip b").textContent,
    detail: mark.querySelector(".pr-video-tip span").textContent,
  }));
  const length = () => (Number.isFinite(video.duration) && video.duration) || Number(root.dataset.seconds);

  // the player takes over from the browser's controls
  video.controls = false;
  root.classList.add("is-ready");

  function update() {
    const at = video.currentTime;
    progress.style.width = `${(at / length()) * 100}%`;
    time.textContent = `${clock(at)} / ${clock(length())}`;
    track.setAttribute("aria-valuenow", String(Math.round(at)));
    track.setAttribute("aria-valuetext", clock(at));
    const now = chapters.filter((c) => c.at <= at + 0.05).pop() ?? chapters[0];
    if (chapter.firstElementChild.textContent !== now.title) {
      chapter.firstElementChild.textContent = now.title;
      chapter.lastElementChild.textContent = now.detail;
    }
    root.classList.toggle("is-playing", !video.paused && !video.ended);
    root.classList.toggle("is-muted", video.muted);
    toggle.setAttribute("aria-label", video.paused ? "Play" : "Pause");
    mute.setAttribute("aria-label", video.muted ? "Unmute" : "Mute");
  }
  // between the video's own reports, so that the timeline moves evenly
  function follow() {
    update();
    if (!video.paused && !video.ended) requestAnimationFrame(follow);
  }
  const seek = (to) => {
    video.currentTime = Math.min(Math.max(0, to), length() - 0.05);
    update();
  };
  const play = () => {
    if (video.ended) video.currentTime = 0;
    video.play().catch(() => {});
  };
  const playOrPause = () => (video.paused || video.ended ? play() : video.pause());

  // the move between the video's place in the page and its place over it
  const fly = (from, to, backwards) => {
    if (calm) return Promise.resolve();
    const away = {
      transformOrigin: "0 0",
      transform: `translate(${from.left - to.left}px, ${from.top - to.top}px) scale(${from.width / to.width})`,
    };
    const home = { transformOrigin: "0 0", transform: "none" };
    const timing = {
      duration: 320,
      easing: "cubic-bezier(0.2, 0.8, 0.2, 1)",
      direction: backwards ? "reverse" : "normal",
    };
    backdrop.animate([{ opacity: 0 }, { opacity: 1 }], timing);
    return dialog.animate([away, home], timing).finished;
  };
  function expand(at) {
    if (open && open !== api) open.collapse();
    if (!root.classList.contains("is-expanded")) {
      const from = frame.getBoundingClientRect();
      root.classList.add("is-expanded");
      // In the browser's top layer, so that it lies over the page's header
      // and sidebars whatever they are stacked in. The attribute is set only
      // now: an element that has it is hidden until it is shown.
      if (shell.showPopover) {
        shell.setAttribute("popover", "manual");
        shell.showPopover();
      }
      document.documentElement.style.overflow = "hidden";
      open = api;
      fly(from, dialog.getBoundingClientRect(), false);
      toggle.focus({ preventScroll: true });
    }
    if (at !== undefined) seek(at);
    play();
  }
  async function collapse() {
    if (!root.classList.contains("is-expanded")) return;
    video.pause();
    if (document.fullscreenElement) await document.exitFullscreen().catch(() => {});
    await fly(frame.getBoundingClientRect(), dialog.getBoundingClientRect(), true).catch(() => {});
    if (shell.hasAttribute("popover")) {
      shell.hidePopover();
      shell.removeAttribute("popover");
    }
    root.classList.remove("is-expanded");
    document.documentElement.style.overflow = "";
    if (open === api) open = null;
    start.focus({ preventScroll: true });
  }
  const fullScreen = () => {
    if (document.fullscreenElement) document.exitFullscreen();
    else (dialog.requestFullscreen || dialog.webkitRequestFullscreen)?.call(dialog);
  };
  const api = { collapse, playOrPause, seek, fullScreen, video };

  start.addEventListener("click", () => expand());
  root
    .querySelectorAll(".pr-video-jump")
    .forEach((jump) => jump.addEventListener("click", () => expand(Number(jump.dataset.at))));
  root.querySelectorAll("[data-video-close]").forEach((close) => close.addEventListener("click", collapse));
  toggle.addEventListener("click", playOrPause);
  video.addEventListener("click", () => root.classList.contains("is-expanded") && playOrPause());
  mute.addEventListener("click", () => {
    video.muted = !video.muted;
  });
  root.querySelector(".pr-video-full").addEventListener("click", fullScreen);
  for (const name of ["play", "pause", "ended", "timeupdate", "volumechange", "loadedmetadata", "seeked"])
    video.addEventListener(name, update);
  video.addEventListener("play", follow);

  // the timeline: a click or a drag goes to that moment, a mark to its chapter
  const momentAt = (event) => {
    const box = track.getBoundingClientRect();
    return (Math.min(Math.max(0, event.clientX - box.left), box.width) / box.width) * length();
  };
  track.addEventListener("pointerdown", (event) => {
    const mark = event.target.closest(".pr-video-mark");
    if (mark) return seek(Number(mark.dataset.at));
    track.setPointerCapture(event.pointerId);
    root.classList.add("is-seeking");
    seek(momentAt(event));
  });
  track.addEventListener("pointermove", (event) => {
    if (root.classList.contains("is-seeking")) seek(momentAt(event));
  });
  for (const name of ["pointerup", "pointercancel"])
    track.addEventListener(name, () => root.classList.remove("is-seeking"));
  update();
}

document.querySelectorAll("[data-video]").forEach(setUpVideo);

// the keys of the open video: the ones a player is expected to answer
document.addEventListener("keydown", (event) => {
  if (!open || event.metaKey || event.ctrlKey || event.altKey) return;
  const { video } = open;
  const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
  if (key === "Escape") {
    // the browser leaves the whole screen on this key itself
    if (!document.fullscreenElement) open.collapse();
  } else if (key === " " || key === "k") {
    // a button with the focus takes the space bar itself
    if (key === " " && event.target.closest("button")) return;
    open.playOrPause();
  } else if (key === "ArrowLeft") open.seek(video.currentTime - 5);
  else if (key === "ArrowRight") open.seek(video.currentTime + 5);
  else if (key === "m") video.muted = !video.muted;
  else if (key === "f") open.fullScreen();
  else return;
  event.preventDefault();
});
