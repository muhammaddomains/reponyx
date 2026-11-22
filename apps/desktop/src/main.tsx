import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getSettings } from "@reponyx/api-client";
import App from "./App";
import { applyTheme, parseTheme } from "./lib/theme";
import "./styles/index.css";

// Apply the saved theme before the first paint so the app does not
// flash the wrong theme. A missing backend (plain-browser preview)
// just falls back to the system theme.
getSettings()
  .then((settings) => applyTheme(parseTheme(settings.theme)))
  .catch(() => applyTheme("system"));

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
