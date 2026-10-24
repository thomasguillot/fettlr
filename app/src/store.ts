import { getCurrentWindow } from "@tauri-apps/api/window";
import { t } from "i18next";
import { create } from "zustand";
import { useEffect, useRef } from "react";
import {
  commands,
  events,
  type AppUpdateProgress,
  type UiAppUpdate,
  type UiColorMode,
  type UiError,
  type UiPalette,
  type UiPrefs,
  type UiSnapshot,
  type UiUpdateStage,
} from "./bindings";
import { type SectionId, needsUpdatingTotal } from "./sections";
import { failureKey } from "./updateFailures";

export type DetailSection =
  | "skills"
  | "subagents"
  | "plugins"
  | "marketplaces"
  | "mcp"
  | "connectors"
  | "browse";

// Browse is a pane inside the plugins section rather than a section of its own, so it can hold a
// selection but is not somewhere search can route to.
type NavigableSection = Exclude<DetailSection, "browse">;

export type RevealRequest = { section: SectionId; id: string; seq: number };

export type DetailPane = { id: string | null; open: boolean };

/** The surface layered over the main content: the search palette. */
export type Panel = "search";

/**
 * Where Fettlr's own update stands. It lives in the store rather than in the settings screen so
 * that a check started from the menu bar, or on launch, is the one the screen shows when it opens,
 * and so the sidebar can badge a waiting update without the settings screen being mounted at all.
 */
export type AppUpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "current" }
  | { kind: "available"; update: UiAppUpdate }
  | { kind: "installing" }
  | { kind: "installed" }
  | { kind: "failed" };

export type CommandResult<T> =
  | { status: "ok"; data: T }
  | { status: "error"; error: UiError };

export type UpdateRun = {
  phase: "running" | "finished";
  items: Record<string, UiUpdateStage>;
};

type FettlrState = {
  snapshot: UiSnapshot | null;
  busy: boolean;
  /** How many commands are in flight. `busy` is this being non-zero. */
  inFlight: number;
  error: UiError | null;
  selection: SectionId;
  detail: Record<DetailSection, DetailPane>;
  updateRun: UpdateRun | null;
  prefs: UiPrefs | null;
  panel: Panel | null;
  appUpdate: AppUpdateState;
  download: AppUpdateProgress | null;
  select: (section: SectionId) => void;
  openDetail: (section: DetailSection, id: string) => void;
  closeDetail: (section: DetailSection) => void;
  /** Switches to a section and opens one of its items in the same tick, for search-style routing. */
  navigateToItem: (section: NavigableSection, id: string) => void;
  revealRequest: RevealRequest | null;
  /** Asks a section to make an item it already selected visible, clearing filters that hide it. */
  reveal: (section: SectionId, id: string) => void;
  clearError: () => void;
  setPanel: (panel: Panel | null) => void;
  /** Bumped by Edit > Find; whichever screen is mounted focuses its filter on the change. */
  findRequest: number;
  requestFind: () => void;
  loadPrefs: () => Promise<void>;
  savePrefs: (next: UiPrefs) => Promise<boolean>;
  checkAppUpdate: (userAsked: boolean) => Promise<void>;
  installAppUpdate: () => Promise<void>;
  checkContentUpdatesInBackground: () => Promise<void>;
  dispatch: <T>(command: Promise<CommandResult<T>>) => Promise<T | undefined>;
  runUpdateAll: () => Promise<void>;
};

// bindings.ts re-throws when the rejection is an Error rather than a typed UiError, which is what
// an IPC-level failure looks like. Turning that back into an error result keeps every caller's
// existing `result.status` branch as the single exit, so none of them can strand a transient state
// like `checking` or `installing`, both of which leave the reader no control to press.
function ipcFailure(cause: unknown): { status: "error"; error: UiError } {
  return {
    status: "error",
    error: {
      message: t("error.unreachableBackend"),
      detail: cause instanceof Error ? cause.message : String(cause),
    },
  };
}

const CLOSED_DETAIL: Record<DetailSection, DetailPane> = {
  skills: { id: null, open: false },
  subagents: { id: null, open: false },
  plugins: { id: null, open: false },
  marketplaces: { id: null, open: false },
  mcp: { id: null, open: false },
  connectors: { id: null, open: false },
  browse: { id: null, open: false },
};

export const useFettlr = create<FettlrState>((set, get) => ({
  snapshot: null,
  busy: false,
  inFlight: 0,
  error: null,
  selection: "updates",
  detail: CLOSED_DETAIL,
  updateRun: null,
  prefs: null,
  panel: null,
  findRequest: 0,
  revealRequest: null,
  appUpdate: { kind: "idle" },
  download: null,
  select: (section) => set({ selection: section }),
  openDetail: (section, id) =>
    set((state) => ({ detail: { ...state.detail, [section]: { id, open: true } } })),
  closeDetail: (section) =>
    set((state) => ({
      detail: { ...state.detail, [section]: { id: state.detail[section].id, open: false } },
    })),
  navigateToItem: (section, id) =>
    set((state) => ({
      selection: section,
      detail: { ...state.detail, [section]: { id, open: true } },
      revealRequest: { section, id, seq: (state.revealRequest?.seq ?? 0) + 1 },
    })),
  reveal: (section, id) =>
    set((state) => ({ revealRequest: { section, id, seq: (state.revealRequest?.seq ?? 0) + 1 } })),
  clearError: () =>
    set((state) => ({
      error: null,
      ...(state.updateRun?.phase === "finished" ? { updateRun: null } : {}),
    })),
  setPanel: (panel) => set({ panel }),
  requestFind: () => set((state) => ({ findRequest: state.findRequest + 1 })),
  loadPrefs: async () => {
    const result = await commands.getPrefs().catch(ipcFailure);
    if (result.status === "error") {
      set({ error: result.error });
      return;
    }
    set({ prefs: result.data });
  },
  savePrefs: async (next) => {
    const previous = get().prefs;
    set({ prefs: next });
    const result = await commands.setPrefs(next).catch(ipcFailure);
    if (result.status === "error") {
      if (get().prefs === next) set({ prefs: previous });
      set({ error: result.error });
      return false;
    }
    if (get().prefs === next) set({ prefs: result.data });
    if (result.data.binaryOverride !== (previous?.binaryOverride ?? null)) {
      void get().checkContentUpdatesInBackground();
    }
    return true;
  },
  // Never routed through dispatch: a check the reader did not ask for must not raise the error
  // dialog over whatever they were doing, and one they did ask for belongs beside the button they
  // pressed. The shell logs the reason either way.
  checkAppUpdate: async (userAsked) => {
    const { kind } = get().appUpdate;
    if (kind === "checking" || kind === "installing" || kind === "installed") return;
    set({ appUpdate: { kind: "checking" } });
    const result = await commands.checkAppUpdate().catch(ipcFailure);
    if (result.status === "error") {
      set({ appUpdate: userAsked ? { kind: "failed" } : { kind: "idle" } });
      return;
    }
    set({
      appUpdate: result.data ? { kind: "available", update: result.data } : { kind: "current" },
    });
  },
  // Always asked for, so a failure here does go to the error dialog: a refused signature or a
  // half-written package is something the reader has to be able to read in full. The row itself
  // goes back to neutral rather than borrowing the failed-check line, which would be a lie.
  installAppUpdate: async () => {
    if (get().appUpdate.kind !== "available") return;
    set({ appUpdate: { kind: "installing" }, download: null });
    const result = await commands.installAppUpdate().catch(ipcFailure);
    if (result.status === "error") {
      set({ appUpdate: { kind: "idle" }, download: null, error: result.error });
      return;
    }
    set({ appUpdate: { kind: "installed" }, download: null });
  },
  // Not routed through dispatch, for the same reason as checkAppUpdate: an interval tick the reader
  // did not ask for must not disable the UI or raise the error dialog. The Refresh button keeps
  // using dispatch(commands.checkUpdates()) so an explicit request still surfaces its own failure.
  checkContentUpdatesInBackground: async () => {
    const result = await commands.checkUpdates().catch(ipcFailure);
    if (result.status === "error") {
      console.warn("a background content check failed", result.error);
      return;
    }
    set({ snapshot: result.data });
  },
  runUpdateAll: async () => {
    if (get().updateRun?.phase === "running") return;
    set({ updateRun: { phase: "running", items: {} } });
    const snapshot = await get().dispatch(commands.updateAll());
    if (!snapshot) {
      set({ updateRun: null });
      return;
    }
    const failures = snapshot.updateFailures;
    // update_all's own refresh() carries the previous update report forward untouched, so re-check
    // here or the needs-updating list and sidebar badges stay stale after a real success.
    await get().dispatch(commands.checkUpdates());
    const current = get().updateRun;
    if (failures.length === 0 || !current) {
      set({ updateRun: null });
    } else {
      set({
        updateRun: { ...current, phase: "finished" },
        error: {
          message: t("updateAll.failed", { count: failures.length }),
          detail: failures
            .map((failure) => t(failureKey(failure.kind), { name: failure.name, error: failure.error }))
            .join("\n\n"),
        },
      });
    }
  },
  dispatch: async (command) => {
    set((state) => ({ inFlight: state.inFlight + 1, busy: true }));
    try {
      const result = await command;
      if (result.status === "error") {
        set({ error: result.error });
        return undefined;
      }
      if (isSnapshot(result.data)) {
        set({ snapshot: result.data });
      }
      return result.data;
    } catch (cause) {
      set({ error: { message: describe(cause), detail: null } });
      return undefined;
    } finally {
      // Counted, not a flag: a short command finishing during a long one would otherwise
      // declare the long one done, re-enabling every control.
      set((state) => {
        const inFlight = Math.max(state.inFlight - 1, 0);
        return {
          inFlight,
          busy: inFlight > 0,
        };
      });
    }
  },
}));

export function useDetailPane(section: DetailSection) {
  const pane = useFettlr((state) => state.detail[section]);
  const openDetail = useFettlr((state) => state.openDetail);
  const closeDetail = useFettlr((state) => state.closeDetail);
  return {
    id: pane.id,
    open: pane.open,
    select: (id: string) => openDetail(section, id),
    close: () => closeDetail(section),
  };
}

export function useFindRequest(onFind: () => void) {
  const findRequest = useFettlr((state) => state.findRequest);
  const callback = useRef(onFind);
  useEffect(() => {
    callback.current = onFind;
  });
  const seen = useRef(findRequest);
  useEffect(() => {
    if (findRequest === seen.current) return;
    seen.current = findRequest;
    callback.current();
  }, [findRequest]);
}

// Every mutation answers with the snapshot it produced, so the store never has to be told which
// commands change the world: it keeps whatever came back that is one.
function isSnapshot(value: unknown): value is UiSnapshot {
  return (
    typeof value === "object" &&
    value !== null &&
    "cliAvailable" in value &&
    "mcpServers" in value
  );
}

function describe(cause: unknown): string {
  if (cause instanceof Error) return cause.message;
  if (typeof cause === "string") return cause;
  return JSON.stringify(cause);
}

export function currentColorMode(state: { prefs: UiPrefs | null }): UiColorMode {
  return state.prefs?.colorMode ?? "system";
}

export function currentPalette(state: { prefs: UiPrefs | null }): UiPalette {
  return state.prefs?.palette ?? "default";
}

export async function startEventBridge(): Promise<() => void> {
  const stopStateChanged = await events.stateChanged.listen((event) => {
    useFettlr.setState({ snapshot: event.payload });
  });
  const stopUpdateProgress = await events.updateProgress.listen((event) => {
    const run = useFettlr.getState().updateRun;
    if (!run) return;
    useFettlr.setState({
      updateRun: {
        ...run,
        items: { ...run.items, [`${event.payload.kind}:${event.payload.id}`]: event.payload.stage },
      },
    });
  });
  const stopAppUpdateProgress = await events.appUpdateProgress.listen((event) => {
    useFettlr.setState({ download: event.payload });
  });
  return () => {
    stopStateChanged();
    stopUpdateProgress();
    stopAppUpdateProgress();
  };
}

let badgedTotal: number | undefined;
useFettlr.subscribe((state) => {
  const total = needsUpdatingTotal(state.snapshot);
  if (total === badgedTotal) return;
  badgedTotal = total;
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;
  void getCurrentWindow()
    .setBadgeCount(total > 0 ? total : undefined)
    .catch(() => undefined);
});
