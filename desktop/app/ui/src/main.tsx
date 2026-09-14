import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { applySavedTheme, cachedAppearance } from "./lib/theme";
import { applyTextSize } from "./state/settings";
import "./theme.css";

applySavedTheme();
applyTextSize(cachedAppearance().text_size);

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
