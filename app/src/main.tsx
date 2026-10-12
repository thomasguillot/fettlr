import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import type { UiPalette } from "./bindings";
import i18n from "./i18n";
import { installAppMenu } from "./menu";
import { currentColorMode, currentPalette, startEventBridge, useFettlr } from "./store";
import "./styles.css";

const PALETTE_CSS: Record<UiPalette, string> = {
  default: "default",
  highContrast: "high-contrast",
  dracula: "dracula",
};

void startEventBridge();
void useFettlr.getState().loadPrefs();
void installAppMenu(i18n.t);
i18n.on("languageChanged", () => void installAppMenu(i18n.t));

const container = document.getElementById("root");
if (!container) throw new Error("the root element is missing from index.html");

const systemDark = window.matchMedia("(prefers-color-scheme: dark)");
let appliedDark: boolean | undefined;
let appliedPalette: string | undefined;
const applyTheme = () => {
  const state = useFettlr.getState();
  const colorMode = currentColorMode(state);
  const dark = colorMode === "dark" || (colorMode === "system" && systemDark.matches);
  const palette = PALETTE_CSS[currentPalette(state)];
  // useFettlr.subscribe fires on every store change, not just appearance changes; skip when nothing resolved differently.
  if (dark === appliedDark && palette === appliedPalette) return;
  const isInitial = appliedDark === undefined;
  appliedDark = dark;
  appliedPalette = palette;
  const applyDom = () => {
    document.documentElement.classList.toggle("dark", dark);
    document.documentElement.dataset.palette = palette;
  };

  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (isInitial || reducedMotion || typeof document.startViewTransition !== "function") {
    applyDom();
    return;
  }
  document.startViewTransition(applyDom);
};
applyTheme();
systemDark.addEventListener("change", applyTheme);
useFettlr.subscribe(applyTheme);

const root = createRoot(container);

if (import.meta.env.DEV && window.location.hash === "#gallery") {
  void import("./dev/Gallery").then(({ Gallery }) =>
    root.render(
      <StrictMode>
        <Gallery />
      </StrictMode>,
    ),
  );
} else {
  root.render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
