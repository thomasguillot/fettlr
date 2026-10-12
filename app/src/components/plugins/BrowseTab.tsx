import {
  ScrollArea,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
  cn,
  insetFocusRing,
} from "@fettlr/design-system";
import { Fragment, useMemo } from "react";
import { RiCheckLine, RiFileDamageLine, RiPuzzleLine } from "@remixicon/react";
import { useTranslation } from "react-i18next";

import { commands, type UiPlugin, type UiProject } from "../../bindings";
import { useDetailPane, useFettlr } from "../../store";
import { ANY } from "../FilterMenu";
import { SidecarLayout, contentFrame, framedTable, screenColumn, stickyHeaderScrollArea, capsHeading, groupHeading, stickyTableHeader } from "../screen";
import { useSidecarRows } from "../useSidecarRows";
import { BrowsePluginDetail } from "./BrowsePluginDetail";
import { EmptyState } from "../EmptyState";

const EMPTY_AVAILABLE: UiPlugin[] = [];
const EMPTY_PROJECTS: UiProject[] = [];

function dedupeByPluginId(list: UiPlugin[]): UiPlugin[] {
  const seen = new Map<string, UiPlugin>();
  for (const entry of list) {
    if (!seen.has(entry.pluginId)) seen.set(entry.pluginId, entry);
  }
  return Array.from(seen.values());
}

export function BrowseTab({ query, marketplace }: { query: string; marketplace: string }) {
  const { t } = useTranslation();
  const snapshot = useFettlr((state) => state.snapshot);
  const dispatch = useFettlr((state) => state.dispatch);
  const busy = useFettlr((state) => state.busy);
  const pane = useDetailPane("browse");

  const availablePlugins = snapshot?.availablePlugins ?? EMPTY_AVAILABLE;
  const projects = snapshot?.projects ?? EMPTY_PROJECTS;

  const installedIds = useMemo(
    () => new Set((snapshot?.plugins ?? []).map((plugin) => plugin.pluginId)),
    [snapshot],
  );

  const normalizedQuery = query.trim().toLowerCase();

  const uninstalled = useMemo(
    () => dedupeByPluginId(availablePlugins).filter((entry) => !installedIds.has(entry.pluginId)),
    [availablePlugins, installedIds],
  );

  const groups = useMemo(() => {
    const filtered = uninstalled.filter((entry) => {
      if (marketplace !== ANY && entry.marketplaceName !== marketplace) return false;
      if (!normalizedQuery) return true;
      return (
        entry.name.toLowerCase().includes(normalizedQuery) ||
        (entry.description?.toLowerCase().includes(normalizedQuery) ?? false)
      );
    });
    const byMarketplace = new Map<string, UiPlugin[]>();
    for (const entry of filtered) {
      const list = byMarketplace.get(entry.marketplaceName) ?? [];
      list.push(entry);
      byMarketplace.set(entry.marketplaceName, list);
    }
    return Array.from(byMarketplace.entries())
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([marketplaceName, entries]) => ({
        marketplaceName,
        entries: entries.sort((a, b) => a.name.localeCompare(b.name)),
      }));
  }, [uninstalled, normalizedQuery, marketplace]);

  const entries = useMemo(() => groups.flatMap((group) => group.entries), [groups]);

  const nothingExistsYet = availablePlugins.length === 0;
  const allInstalled = !nothingExistsYet && uninstalled.length === 0;
  const noResults = groups.length === 0;

  const { bodyRef, open, selectedId, closeDetail, rowProps } = useSidecarRows(
    pane,
    entries.map((entry) => entry.pluginId),
  );
  const selectedEntry = selectedId ? (entries.find((entry) => entry.pluginId === selectedId) ?? null) : null;

  const columns = {
    name: "w-[40%]",
    description: "w-[46%]",
    version: "w-[14%]",
  };

  return (
    <SidecarLayout
      open={open}
      label={t("plugins.detail.paneLabel")}
      onClose={closeDetail}
      detail={
        <BrowsePluginDetail
          entry={selectedEntry}
          projects={projects}
          busy={busy}
          onInstall={(pluginId, scope) => {
            // The plugin leaves this list the moment it installs, so the pane's remembered id would
            // otherwise reopen it the day the reader uninstalls it again.
            void dispatch(commands.installPlugin(pluginId, scope)).then((result) => {
              if (result) pane.close();
            });
          }}
        />
      }
    >
      <div className={screenColumn}>
        {noResults ? (
          <EmptyState
            icon={nothingExistsYet ? RiPuzzleLine : allInstalled ? RiCheckLine : RiFileDamageLine}
            title={
              nothingExistsYet
                ? t("plugins.browse.empty")
                : allInstalled
                  ? t("plugins.browse.allInstalled")
                  : t("plugins.browse.noResults")
            }
            body={
              nothingExistsYet
                ? t("plugins.browse.emptyBody")
                : allInstalled
                  ? t("plugins.browse.allInstalledBody")
                  : t("plugins.browse.noResultsBody")
            }
          />
        ) : (
          <div className={contentFrame}>
            <ScrollArea className={cn("min-h-0", stickyHeaderScrollArea)}>
              <Table role="grid" className={framedTable} containerClassName="overflow-x-visible">
                <TableHeader className={stickyTableHeader}>
                  <TableRow>
                    <TableHead className={cn("truncate", columns.name)}>{t("plugins.columnName")}</TableHead>
                    <TableHead className={cn("truncate", columns.description)}>
                      {t("plugins.browse.columnDescription")}
                    </TableHead>
                    <TableHead className={cn("truncate", columns.version)}>
                      {t("plugins.columnVersion")}
                    </TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody ref={bodyRef}>
                  {groups.map((group) => (
                    <Fragment key={group.marketplaceName}>
                      {/* Sticks just under the h-8 header row; the next heading slides over it as it arrives. */}
                      <TableRow>
                        <TableCell
                          colSpan={3}
                          className={cn(groupHeading, capsHeading, "sticky top-8 z-5")}
                        >
                          {group.marketplaceName}
                        </TableCell>
                      </TableRow>
                      {group.entries.map((entry) => {
                        return (
                          <TableRow
                            key={entry.pluginId}
                            {...rowProps(entry.pluginId)}
                            className={cn("hover:bg-bg-subtle", insetFocusRing)}
                          >
                            <TableCell className={cn("truncate font-medium text-text", columns.name)}>
                              {entry.name}
                            </TableCell>
                            <TableCell className={cn("truncate text-text-muted", columns.description)}>
                              {entry.description}
                            </TableCell>
                            <TableCell className={cn("truncate text-text-muted", columns.version)}>
                              {entry.version}
                            </TableCell>
                          </TableRow>
                        );
                      })}
                    </Fragment>
                  ))}
                </TableBody>
              </Table>
            </ScrollArea>
          </div>
        )}
      </div>
    </SidecarLayout>
  );
}
