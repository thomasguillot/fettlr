import {
  Button,
  ScrollArea,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
  cn,
} from "@fettlr/design-system";
import { RiCheckboxCircleLine, RiLoader4Line, RiPuzzleLine, RiRefreshLine } from "@remixicon/react";
import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import { commands, type UiUpdateState } from "../bindings";
import { sectionCount, sections, staleIds } from "../sections";
import { useFettlr } from "../store";
import { useUpdateAction } from "../useUpdateAction";
import { describeUpdateState } from "../updateState";
import { EmptyState } from "./EmptyState";
import { StaleBadge } from "./badges";
import { ScreenShell, contentFrame, formatTimestamp, framedTable, screenColumn, stickyHeaderScrollArea, stickyTableHeader } from "./screen";
import { UpdateAllSheet } from "./UpdateAllSheet";
import { repinMcpServer } from "./mcp/mcpConfig";

type UpdateRow = {
  id: string;
  kind: "marketplace" | "plugin" | "skill" | "mcpServer";
  name: string;
  detail: string;
  action: ((id: string) => void) | null;
  actionKey: string;
};

export function UpdatesScreen() {
  const { t } = useTranslation();
  const snapshot = useFettlr((state) => state.snapshot);
  const dispatch = useFettlr((state) => state.dispatch);
  const busy = useFettlr((state) => state.busy);
  const navigateToItem = useFettlr((state) => state.navigateToItem);
  const warnings = snapshot?.warnings ?? [];
  const [refreshing, setRefreshing] = useState(false);
  const { run, running: updatingId } = useUpdateAction();

  const handleRefresh = async () => {
    setRefreshing(true);
    try {
      const refreshed = await dispatch(commands.refresh());
      if (refreshed) await dispatch(commands.checkUpdates());
    } finally {
      setRefreshing(false);
    }
  };

  const describeState = useCallback(
    (state: UiUpdateState | undefined): string => describeUpdateState(state, t) ?? "",
    [t],
  );

  const installedTotal = useMemo(
    () =>
      sections
        .filter((section) => section.id !== "connectors")
        .reduce((total, section) => total + sectionCount(snapshot, section), 0),
    [snapshot],
  );

  const rows = useMemo<UpdateRow[]>(() => {
    if (!snapshot?.updateReport) return [];
    const report = snapshot.updateReport;
    const staleMarketplaces = staleIds(report, "marketplaces");
    const stalePlugins = staleIds(report, "plugins");
    const staleSkills = staleIds(report, "skills");
    const staleServers = staleIds(report, "mcpServers");
    const result: UpdateRow[] = [];

    for (const marketplace of snapshot.marketplaces) {
      if (!staleMarketplaces.has(marketplace.name)) continue;
      result.push({
        id: `marketplace:${marketplace.name}`,
        kind: "marketplace",
        name: marketplace.name,
        detail: describeState(report.marketplaces[marketplace.name]),
        action: (id: string) => void run(id, () => dispatch(commands.updateMarketplace(marketplace.name))),
        actionKey: "common.update",
      });
    }

    for (const plugin of snapshot.plugins) {
      if (!stalePlugins.has(plugin.id)) continue;
      result.push({
        id: `plugin:${plugin.id}`,
        kind: "plugin",
        name: plugin.name,
        detail: describeState(report.plugins[plugin.id]),
        action:
          plugin.scope.kind === "other"
            ? null
            : (id: string) => void run(id, () => dispatch(commands.updatePlugin(plugin.pluginId, plugin.scope))),
        actionKey: "common.update",
      });
    }

    for (const skill of snapshot.skills) {
      if (!staleSkills.has(skill.id)) continue;
      result.push({
        id: `skill:${skill.id}`,
        kind: "skill",
        name: skill.title ?? skill.name,
        detail: describeState(report.skills[skill.id]),
        action: (id: string) => void run(id, () => dispatch(commands.pullSkill(skill.id))),
        actionKey: "common.update",
      });
    }

    for (const server of snapshot.mcpServers) {
      if (!staleServers.has(server.id)) continue;
      const state = report.mcpServers[server.id];
      const latest = state?.kind === "updateAvailable" ? state.latest : null;
      const origin = server.origin;
      let action: ((id: string) => void) | null = null;
      let actionKey = "common.update";
      if (latest !== null && origin.kind !== "plugin") {
        action = (id: string) => void run(id, () => repinMcpServer(dispatch, server, latest));
      } else if (origin.kind === "plugin") {
        const owner = snapshot.plugins.find((plugin) => origin.kind === "plugin" && plugin.pluginId === origin.pluginId) ?? null;
        if (owner) {
          action = () => navigateToItem("plugins", owner.id);
          actionKey = "updates.viewPlugin";
        }
      }
      result.push({
        id: `mcpServer:${server.id}`,
        kind: "mcpServer",
        name: server.name,
        detail: describeState(state),
        action,
        actionKey,
      });
    }

    return result;
  }, [snapshot, dispatch, run, describeState, navigateToItem]);

  const updateAllTotal = useMemo(() => rows.filter((row) => row.kind !== "mcpServer" && row.action !== null).length, [rows]);
  const checked = snapshot?.updateReport != null;

  const columns = {
    name: "w-[40%]",
    kind: "w-[18%]",
    status: "w-[27%]",
    action: "w-[15%]",
  };

  return (
    <ScreenShell
      toolbar={
        <p className="truncate text-sm text-text-muted">
          {snapshot?.updateReport
            ? t("updates.lastChecked", { time: formatTimestamp(snapshot.updateReport.checkedAt) })
            : t("updates.neverChecked")}
        </p>
      }
      actions={
        <>
          <Button type="button" variant="secondary" onClick={() => void handleRefresh()} disabled={busy}>
            {refreshing ? (
              <RiLoader4Line className="size-4 animate-spin" aria-hidden="true" />
            ) : (
              <RiRefreshLine className="size-4" aria-hidden="true" />
            )}
            {refreshing ? t("common.refreshing") : t("common.refresh")}
          </Button>
          <UpdateAllSheet staleTotal={updateAllTotal} />
        </>
      }
    >
      <div className={screenColumn}>
        {rows.length === 0 && warnings.length === 0 ? (
          installedTotal === 0 ? (
            <EmptyState
              icon={RiPuzzleLine}
              title={t("updates.nothingInstalled")}
              body={t("updates.nothingInstalledBody")}
            />
          ) : (
            <EmptyState
              icon={RiCheckboxCircleLine}
              title={t(checked ? "updates.emptyTitle" : "updates.neverChecked")}
              body={t(checked ? "updates.emptyBody" : "updates.uncheckedBody")}
            />
          )
        ) : rows.length > 0 ? (
          <div className={contentFrame}>
            <ScrollArea className={cn("min-h-0", stickyHeaderScrollArea)}>
              <Table className={framedTable} containerClassName="overflow-x-visible">
                <TableHeader className={stickyTableHeader}>
                  <TableRow>
                    <TableHead className={cn("truncate", columns.name)}>{t("updates.item")}</TableHead>
                    <TableHead className={cn("truncate", columns.kind)}>{t("updates.kind")}</TableHead>
                    <TableHead className={cn("truncate", columns.status)}>{t("updates.status")}</TableHead>
                    <TableHead className={cn("text-right", columns.action)}>{t("updates.action")}</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {rows.map((row) => (
                    <TableRow key={row.id}>
                      <TableCell className="truncate font-medium text-text">{row.name}</TableCell>
                      <TableCell className={cn("truncate text-text-muted", columns.kind)}>
                        {t(`updates.kind${capitalize(row.kind)}`)}
                      </TableCell>
                      <TableCell className={cn("overflow-hidden", columns.status)}>
                        <StaleBadge>{row.detail}</StaleBadge>
                      </TableCell>
                      <TableCell className={cn("text-right", columns.action)}>
                        <Button
                          type="button"
                          variant="secondary"
                          size="sm"
                          disabled={busy || updatingId !== null || row.action === null}
                          onClick={() => row.action?.(row.id)}
                          className="relative"
                        >
                          <span className={cn(updatingId === row.id && "invisible")}>
                            {t(row.actionKey)}
                          </span>
                          {updatingId === row.id ? (
                            <>
                              <RiLoader4Line
                                className="absolute inset-0 m-auto size-4 animate-spin"
                                aria-hidden="true"
                              />
                              <span className="sr-only">{t("common.updating")}</span>
                            </>
                          ) : null}
                        </Button>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </ScrollArea>
          </div>
        ) : null}
        <p className="mt-auto pt-6 text-xs text-text-muted">
          {t("common.newSessionsNote")}
        </p>
      </div>
    </ScreenShell>
  );
}

function capitalize(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1);
}
