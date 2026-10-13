import { Button, TooltipProvider, cn } from "@fettlr/design-system";
import { RiErrorWarningLine } from "@remixicon/react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { commands } from "./bindings";
import { EmptyState } from "./components/EmptyState";
import { ErrorDialog } from "./components/ErrorDialog";
import { UpdateFailuresSheet } from "./components/UpdateFailuresSheet";
import { MarketplacesList } from "./components/marketplaces/MarketplacesList";
import { ConnectorsScreen } from "./components/connectors/ConnectorsScreen";
import { McpList } from "./components/mcp/McpList";
import { Onboarding } from "./components/Onboarding";
import { UpdatesScreen } from "./components/UpdatesScreen";
import { PluginsList } from "./components/plugins/PluginsList";
import { SearchPalette } from "./components/search/SearchPalette";
import { SettingsScreen } from "./components/settings/SettingsScreen";
import { Sidebar, sidebarWidths } from "./components/Sidebar";
import { SkillsList } from "./components/skills/SkillsList";
import { SubagentsList } from "./components/subagents/SubagentsList";
import { sections } from "./sections";
import { useFettlr } from "./store";

const HOUR_MS = 60 * 60 * 1000;

export function App() {
  const { t } = useTranslation();
  const snapshot = useFettlr((state) => state.snapshot);
  const selection = useFettlr((state) => state.selection);
  const busy = useFettlr((state) => state.busy);
  const dispatch = useFettlr((state) => state.dispatch);
  const intervalHours = useFettlr((state) => state.prefs?.checkIntervalHours ?? null);
  const autoCheck = useFettlr((state) => state.prefs?.autoCheckForUpdates ?? null);
  const checkAppUpdate = useFettlr((state) => state.checkAppUpdate);
  const checkContentUpdatesInBackground = useFettlr(
    (state) => state.checkContentUpdatesInBackground,
  );

  const [initialRefreshFailed, setInitialRefreshFailed] = useState(false);
  const runInitialRefresh = useCallback(() => {
    void dispatch(commands.refresh()).then((result) => {
      if (result === undefined) setInitialRefreshFailed(true);
    });
  }, [dispatch]);

  useEffect(() => {
    runInitialRefresh();
  }, [runInitialRefresh]);

  const retryInitialRefresh = () => {
    setInitialRefreshFailed(false);
    runInitialRefresh();
  };

  const cliAvailable = snapshot?.cliAvailable ?? false;
  const hasUpdateReport = snapshot?.updateReport != null;

  // Settles once, on success or failure, so a failed check does not retry forever and does not
  // leave the whole window waiting behind the error dialog it raises.
  const [initialCheckSettled, setInitialCheckSettled] = useState(false);
  useEffect(() => {
    if (cliAvailable && !hasUpdateReport && !busy && !initialCheckSettled) {
      void dispatch(commands.checkUpdates()).finally(() => setInitialCheckSettled(true));
    }
  }, [cliAvailable, hasUpdateReport, busy, initialCheckSettled, dispatch]);

  // Held until the update check settles too, not just until the snapshot arrives, so the sidebar's
  // counts and the needs-updating table appear once, fully formed, instead of the
  // up-to-date state flashing before the real table replaces it a beat later.
  const awaitingInitialCheck = cliAvailable && !hasUpdateReport && !initialCheckSettled;
  const showSkeleton = !snapshot || awaitingInitialCheck;

  // The check above covers the launch; this one keeps it running while the window stays open. It
  // is a tick the reader did not ask for, so it stays off dispatch: no busy state, no error dialog.
  useEffect(() => {
    if (!cliAvailable || intervalHours === null) return;
    const timer = window.setInterval(
      () => void checkContentUpdatesInBackground(),
      intervalHours * HOUR_MS,
    );
    return () => window.clearInterval(timer);
  }, [cliAvailable, intervalHours, checkContentUpdatesInBackground]);

  useEffect(() => {
    if (autoCheck !== true) return;
    void checkAppUpdate(false);
  }, [autoCheck, checkAppUpdate]);

  return (
    <TooltipProvider>
      <div className="flex h-screen bg-bg-subtle text-text">
        {!snapshot && initialRefreshFailed ? (
          <main className="flex min-w-0 flex-1 flex-col">
            <EmptyState
              icon={RiErrorWarningLine}
              title={t("error.title")}
              body={t("app.loadFailed")}
              action={
                <Button type="button" onClick={retryInitialRefresh}>
                  {t("app.retry")}
                </Button>
              }
            />
          </main>
        ) : showSkeleton ? (
          <ShellSkeleton />
        ) : (
          <>
            <Sidebar />
            <main className="@container flex min-h-0 min-w-0 flex-1 flex-col animate-shell-reveal">
              {selection === "updates" ? (
                cliAvailable ? (
                  <UpdatesScreen />
                ) : (
                  <Onboarding />
                )
              ) : selection === "skills" ? (
                <SkillsList />
              ) : selection === "subagents" ? (
                <SubagentsList />
              ) : selection === "plugins" ? (
                <PluginsList />
              ) : selection === "marketplaces" ? (
                <MarketplacesList />
              ) : selection === "mcp" ? (
                <McpList />
              ) : selection === "connectors" ? (
                <ConnectorsScreen />
              ) : (
                <SettingsScreen />
              )}
            </main>
          </>
        )}
        <SearchPalette />
        <UpdateFailuresSheet />
        <ErrorDialog />
      </div>
    </TooltipProvider>
  );
}

const rail = "animate-pulse rounded-md bg-text/3";
const pane = "animate-pulse rounded-md bg-bg-subtle";

// A row's ink, not its box: filled bars at the real row height read as a comb, where the rail
// itself is mostly whitespace around an icon and a label. The label takes its width from the one
// it stands in for, so the rows vary the way the real ones do.
function SkeletonRow({ label, collapsed }: { label: string; collapsed: boolean }) {
  return (
    <div
      className={cn(
        "flex h-9 shrink-0 items-center gap-2",
        collapsed ? "w-9 justify-center" : "w-(--sidebar-row) px-2.5",
      )}
    >
      <div className={cn(rail, "size-4 shrink-0 rounded-sm")} />
      {collapsed ? null : (
        <div className={cn("h-3.5", rail)} style={{ width: `${label.length * 7.5}px` }} />
      )}
    </div>
  );
}

// Stands in for the shell the launch check is about to reveal, block for block, so nothing shifts
// when it lands: the sidebar sections, the settings group under them, the toolbar's text and one
// certain button, one block standing in for the list, and the footnote bar under it.
function ShellSkeleton() {
  const { t } = useTranslation();
  const collapsed = useFettlr((state) => state.prefs?.sidebarCollapsed ?? false);
  return (
    <div className="flex h-full min-h-0 w-full" role="status">
      <span className="sr-only">{t("shell.loading")}</span>
      <div
        style={sidebarWidths}
        className={cn(
          "flex shrink-0 flex-col bg-bg-subtle",
          collapsed ? "w-(--sidebar-rail)" : "w-60",
        )}
      >
        <div data-tauri-drag-region className="h-7 shrink-0" />
        <div className="flex flex-1 flex-col items-center gap-5 p-5 pb-7">
          <div aria-hidden="true" className="flex flex-1 flex-col gap-0.5">
            {sections.map((section) => (
              <SkeletonRow key={section.id} label={t(section.labelKey)} collapsed={collapsed} />
            ))}
          </div>
          <div aria-hidden="true" className="flex flex-col gap-2">
            <SkeletonRow label={t("sections.settings")} collapsed={collapsed} />
            <div className="h-px shrink-0 bg-border" />
            <SkeletonRow label={t("sidebar.collapse")} collapsed={collapsed} />
          </div>
        </div>
      </div>
      <div className="@container flex min-h-0 flex-1 flex-col">
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden bg-bg">
          <div
            data-tauri-drag-region
            className="flex h-15 shrink-0 items-center justify-between gap-2 px-6"
          >
            <div aria-hidden="true" className={cn(pane, "h-4 w-48 rounded-sm")} />
            <div aria-hidden="true" className={cn("h-9 w-25 shrink-0", pane)} />
          </div>
          <div aria-hidden="true" className="flex min-h-0 flex-1 flex-col px-6 pb-6">
            <div className={cn(pane, "h-45 rounded-xl")} />
            <div className={cn(pane, "mt-auto h-4 w-56 rounded-sm")} />
          </div>
        </div>
      </div>
    </div>
  );
}
