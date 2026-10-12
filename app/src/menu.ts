import { Menu, MenuItem, PredefinedMenuItem, Submenu } from "@tauri-apps/api/menu";
import type { TFunction } from "i18next";

import type { SectionId } from "./sections";
import { useFettlr, type Panel } from "./store";

const FILTERABLE = new Set<SectionId>(["skills", "subagents", "plugins", "marketplaces", "mcp"]);

function canFind(state: { selection: SectionId; panel: Panel | null }): boolean {
  return FILTERABLE.has(state.selection) && state.panel === null;
}

function modalOpen(): boolean {
  return (
    document.querySelector(
      '[role="dialog"][data-state="open"], [role="alertdialog"][data-state="open"]',
    ) !== null
  );
}

let stopTrackingFind: (() => void) | undefined;

/**
 * Built in the window rather than in the shell so that every label Fettlr writes goes through the
 * same translations the rest of the interface does. Predefined items carry no text of ours: macOS
 * titles and localizes those itself, which is better than anything this file could supply.
 */
export async function installAppMenu(t: TFunction): Promise<void> {
  try {
    const separator = () => PredefinedMenuItem.new({ item: "Separator" });
    const app = await Submenu.new({
      text: t("app.name"),
      items: [
        await MenuItem.new({
          text: t("menu.about"),
          action: () => {
            const { select, setPanel } = useFettlr.getState();
            select("settings");
            setPanel(null);
          },
        }),
        await separator(),
        await MenuItem.new({
          text: t("menu.checkForUpdates"),
          action: () => {
            const { select, setPanel, checkAppUpdate } = useFettlr.getState();
            select("settings");
            setPanel(null);
            void checkAppUpdate(true);
          },
        }),
        await separator(),
        await MenuItem.new({
          text: t("menu.settings"),
          accelerator: "CmdOrCtrl+,",
          action: () => {
            const { select, setPanel } = useFettlr.getState();
            select("settings");
            setPanel(null);
          },
        }),
        await separator(),
        await PredefinedMenuItem.new({ item: "Services" }),
        await separator(),
        await PredefinedMenuItem.new({ item: "Hide" }),
        await PredefinedMenuItem.new({ item: "HideOthers" }),
        await PredefinedMenuItem.new({ item: "ShowAll" }),
        await separator(),
        await PredefinedMenuItem.new({ item: "Quit" }),
      ],
    });

    let findEnabled = canFind(useFettlr.getState());
    const find = await MenuItem.new({
      text: t("menu.find"),
      accelerator: "CmdOrCtrl+F",
      enabled: findEnabled,
      action: () => {
        if (modalOpen()) return;
        useFettlr.getState().requestFind();
      },
    });
    stopTrackingFind?.();
    stopTrackingFind = useFettlr.subscribe((state) => {
      const next = canFind(state);
      if (next === findEnabled) return;
      findEnabled = next;
      void find.setEnabled(next);
    });
    const settled = canFind(useFettlr.getState());
    if (settled !== findEnabled) {
      findEnabled = settled;
      void find.setEnabled(settled);
    }

    const edit = await Submenu.new({
      text: t("menu.edit"),
      items: [
        await PredefinedMenuItem.new({ item: "Undo" }),
        await PredefinedMenuItem.new({ item: "Redo" }),
        await separator(),
        await PredefinedMenuItem.new({ item: "Cut" }),
        await PredefinedMenuItem.new({ item: "Copy" }),
        await PredefinedMenuItem.new({ item: "Paste" }),
        await PredefinedMenuItem.new({ item: "SelectAll" }),
        await separator(),
        find,
        await MenuItem.new({
          text: t("menu.jumpTo"),
          accelerator: "CmdOrCtrl+K",
          action: () => {
            const { panel, setPanel } = useFettlr.getState();
            if (panel !== "search" && modalOpen()) return;
            setPanel(panel === "search" ? null : "search");
          },
        }),
      ],
    });

    const window = await Submenu.new({
      text: t("menu.window"),
      items: [
        await PredefinedMenuItem.new({ item: "Minimize" }),
        await PredefinedMenuItem.new({ item: "Maximize" }),
        await separator(),
        await PredefinedMenuItem.new({ item: "CloseWindow" }),
      ],
    });

    await (await Menu.new({ items: [app, edit, window] })).setAsAppMenu();
  } catch (cause) {
    console.warn("Fettlr could not install its menu bar", cause);
  }
}
