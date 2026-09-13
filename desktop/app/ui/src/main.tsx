import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { applySavedTheme } from "./lib/theme";
import "./theme.css";

applySavedTheme();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
