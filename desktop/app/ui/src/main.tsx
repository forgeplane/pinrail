import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { applySavedTheme, cachedAppearance } from "./lib/theme";
import { applyTextSize } from "./state/settings";
// Inter travels with the app: the weight axis for every script it covers, so
// the window looks the same on a machine that has no Inter of its own and on
// Linux, where the fallback would be whatever sans the distribution ships.
import "@fontsource-variable/inter/wght.css";
import "./theme.css";

applySavedTheme();
applyTextSize(cachedAppearance().text_size);

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
