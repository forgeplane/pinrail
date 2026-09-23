// The docs' screenshots. The whole app, shrunk into the text column, is too
// small to read, so a click opens a picture full size. A figure of several
// shows one at a time with a dot for each; zoomed, arrows move between them.

const slidesOf = (figure) => (figure ? [...figure.querySelectorAll(".wk-slide")] : []);

/* the figure's picture `n`: its slide shown, its dot current */
function show(figure, n) {
  slidesOf(figure).forEach((slide, i) => { slide.hidden = i !== n; });
  figure.querySelectorAll(".wk-dot").forEach((dot, i) => dot.setAttribute("aria-current", String(i === n)));
}

// the picture in the reader's theme, of the ones a button holds
const shown = (button) => [...button.querySelectorAll("img")].find((img) => getComputedStyle(img).display !== "none") || button.querySelector("img");

function openZoom(button) {
  const figure = button.closest(".wk-shots");
  const slides = slidesOf(figure);
  let at = figure ? slides.indexOf(button.closest(".wk-slide")) : 0;

  const dialog = document.createElement("dialog");
  dialog.className = "wk-zoom";
  const img = document.createElement("img");
  const caption = document.createElement("p");
  caption.className = "wk-zoom-caption";
  const counter = document.createElement("span");
  counter.className = "wk-zoom-count";
  const control = (cls, label, text) => {
    const b = document.createElement("button");
    b.type = "button"; b.className = cls; b.setAttribute("aria-label", label); b.textContent = text;
    return b;
  };
  const close = control("wk-zoom-close", "Close", "×");
  const prev = control("wk-zoom-nav wk-zoom-prev", "Previous picture", "‹");
  const next = control("wk-zoom-nav wk-zoom-next", "Next picture", "›");

  const paint = () => {
    const source = figure ? slides[at].querySelector(".wk-shot-open") : button;
    const pic = shown(source);
    img.src = pic.currentSrc || pic.src;
    img.alt = pic.alt || source.querySelector("img").alt || "";
    dialog.setAttribute("aria-label", img.alt || "Screenshot");
    const title = figure ? slides[at].querySelector("figcaption") : button.closest("figure")?.querySelector("figcaption");
    caption.textContent = title ? title.textContent : "";
    caption.hidden = !title;
    counter.textContent = figure ? `${at + 1} / ${slides.length}` : "";
    if (figure) show(figure, at);
  };
  const go = (by) => { if (!figure) return; at = (at + by + slides.length) % slides.length; paint(); };

  dialog.append(close, img, caption);
  if (figure && slides.length > 1) dialog.append(prev, next, counter);
  dialog.addEventListener("click", (event) => {
    if (event.target === prev) { event.stopPropagation(); go(-1); return; }
    if (event.target === next) { event.stopPropagation(); go(1); return; }
    dialog.close();
  });
  dialog.addEventListener("keydown", (event) => {
    if (event.key === "ArrowLeft") { event.preventDefault(); go(-1); }
    if (event.key === "ArrowRight") { event.preventDefault(); go(1); }
  });
  dialog.addEventListener("close", () => {
    dialog.remove();
    (figure ? slides[at].querySelector(".wk-shot-open") : button).focus();
  });
  paint();
  document.body.append(dialog);
  dialog.showModal();
  close.focus();
}

/* A diagram, full size: the one drawn for the reader's theme, on its panel.
   It keeps the figure's class, so its styles (the accent for "you") hold. */
function openDiagram(canvas) {
  const figure = canvas.closest(".wk-mermaid");
  const svg = [...canvas.querySelectorAll("svg")].find((s) => s.getBoundingClientRect().width > 0);
  if (!svg) return;
  const dialog = document.createElement("dialog");
  dialog.className = "wk-zoom";
  dialog.setAttribute("aria-label", figure.getAttribute("aria-label") || "Diagram");
  const panel = document.createElement("div");
  panel.className = "wk-mermaid wk-zoom-diagram";
  panel.append(svg.cloneNode(true));
  const close = document.createElement("button");
  close.type = "button"; close.className = "wk-zoom-close"; close.setAttribute("aria-label", "Close"); close.textContent = "×";
  const title = figure.querySelector("figcaption");
  const caption = document.createElement("p");
  caption.className = "wk-zoom-caption";
  caption.textContent = title ? title.textContent : "";
  caption.hidden = !title;
  dialog.append(close, panel, caption);
  dialog.addEventListener("click", () => dialog.close());
  dialog.addEventListener("close", () => { dialog.remove(); canvas.focus(); });
  document.body.append(dialog);
  dialog.showModal();
  close.focus();
}

document.addEventListener("keydown", (event) => {
  const canvas = event.target.closest && event.target.closest(".wk-mermaid-canvas");
  if (canvas && (event.key === "Enter" || event.key === " ")) { event.preventDefault(); openDiagram(canvas); }
});

document.addEventListener("click", (event) => {
  const canvas = event.target.closest(".wk-mermaid-canvas");
  if (canvas && !canvas.closest(".wk-zoom")) { openDiagram(canvas); return; }
  const dot = event.target.closest(".wk-dot");
  if (dot) { show(dot.closest(".wk-shots"), Number(dot.dataset.dot)); return; }
  const button = event.target.closest(".wk-shot-open");
  if (button) openZoom(button);
});

// the arrow keys move between the dots of a figure
document.addEventListener("keydown", (event) => {
  const dot = event.target.closest && event.target.closest(".wk-dot");
  if (!dot || !["ArrowLeft", "ArrowRight"].includes(event.key)) return;
  const dots = [...dot.parentElement.querySelectorAll(".wk-dot")];
  const n = (dots.indexOf(dot) + (event.key === "ArrowRight" ? 1 : -1) + dots.length) % dots.length;
  event.preventDefault();
  show(dot.closest(".wk-shots"), n);
  dots[n].focus();
});
