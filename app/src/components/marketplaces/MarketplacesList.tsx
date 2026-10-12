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
  insetFocusRing,
} from "@fettlr/design-system";
import { RiAddLine, RiArrowUpCircleLine, RiFileDamageLine, RiLoader4Line, RiStoreLine } from "@remixicon/react";
import { useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { commands, type UiMarketplace } from "../../bindings";
import { staleCount, staleIds } from "../../sections";
import { useDetailPane, useFettlr } from "../../store";
import { useUpdateAction } from "../../useUpdateAction";
import { describeUpdateState } from "../../updateState";
import { StaleBadge } from "../badges";
import { FilterInput } from "../FilterInput";
import { ANY, FilterMenu, type FilterFacet } from "../FilterMenu";
import { ScreenShell, SidecarLayout, contentFrame, framedTable, screenColumn, stickyHeaderScrollArea, stickyTableHeader } from "../screen";
import { useRevealReset } from "../useRevealReset";
import { useSidecarRows } from "../useSidecarRows";
import { marketplaceSearchValue, matchesText } from "../searchText";
import { AddMarketplaceDialog } from "./AddMarketplaceDialog";
import { MarketplaceDetail } from "./MarketplaceDetail";
import { EmptyState } from "../EmptyState";

const EMPTY_MARKETPLACES: UiMarketplace[] = [];

type StatusFilter = typeof ANY | "stale" | "current";

export function MarketplacesList() {
  const { t } = useTranslation();
  const snapshot = useFettlr((state) => state.snapshot);
  const dispatch = useFettlr((state) => state.dispatch);
  const busy = useFettlr((state) => state.busy);
  const pane = useDetailPane("marketplaces");
  const [addOpen, setAddOpen] = useState(false);
  const { run, isRunning } = useUpdateAction();
  const updatingAll = isRunning("all");
  const [sourceFilter, setSourceFilter] = useState<string>(ANY);
  const [statusFilter, setStatusFilter] = useState<StatusFilter>(ANY);
  const [query, setQuery] = useState("");
  const addButtonRef = useRef<HTMLButtonElement>(null);

  const marketplaces = snapshot?.marketplaces ?? EMPTY_MARKETPLACES;
  const plugins = snapshot?.plugins ?? [];
  const availablePlugins = snapshot?.availablePlugins ?? [];
  const updateReport = snapshot?.updateReport ?? null;
  const staleMarketplaces = useMemo(() => staleIds(updateReport, "marketplaces"), [updateReport]);

  const sourceOptions = useMemo(() => {
    const sources = new Set<string>();
    for (const marketplace of marketplaces) sources.add(marketplace.source);
    return Array.from(sources).sort((a, b) => a.localeCompare(b));
  }, [marketplaces]);

  const statusOptions = useMemo(() => {
    const values = new Set<"stale" | "current">();
    for (const marketplace of marketplaces) {
      values.add(staleMarketplaces.has(marketplace.name) ? "stale" : "current");
    }
    return (["stale", "current"] as const).filter((value) => values.has(value));
  }, [marketplaces, staleMarketplaces]);

  const filteredMarketplaces = useMemo(
    () =>
      marketplaces
        .filter(
          (marketplace) =>
            (sourceFilter === ANY || marketplace.source === sourceFilter) &&
            (statusFilter === ANY ||
              (staleMarketplaces.has(marketplace.name) ? "stale" : "current") === statusFilter) &&
            matchesText(marketplaceSearchValue(marketplace), query),
        )
        .sort((a, b) => a.name.localeCompare(b.name)),
    [marketplaces, sourceFilter, statusFilter, staleMarketplaces, query],
  );

  const staleTotal = staleCount(updateReport, "marketplaces");

  const runUpdate = () => run("all", () => dispatch(commands.updateMarketplace(null)));

  useRevealReset(
    "marketplaces",
    filteredMarketplaces.map((marketplace) => marketplace.name),
    () => {
      setSourceFilter(ANY);
      setStatusFilter(ANY);
      setQuery("");
    },
  );

  const { bodyRef, open, closeDetail, rowProps } = useSidecarRows(
    pane,
    filteredMarketplaces.map((marketplace) => marketplace.name),
    { fallbackFocus: () => addButtonRef.current?.focus() },
  );

  const columns = {
    name: "w-[46%]",
    source: "w-[26%]",
    installed: "w-[14%]",
    available: "w-[14%]",
  };

  const facets: FilterFacet[] = [
    {
      id: "source",
      label: t("marketplaces.columnSource"),
      anyLabel: t("filters.anySource"),
      value: sourceFilter,
      onChange: setSourceFilter,
      options: sourceOptions.map((source) => ({ value: source, label: source })),
    },
    {
      id: "status",
      label: t("marketplaces.columnStatus"),
      anyLabel: t("filters.anyStatus"),
      value: statusFilter,
      onChange: (value) => setStatusFilter(value as StatusFilter),
      options: statusOptions.map((status) => ({
        value: status,
        label: status === "stale" ? t("filters.statusStale") : t("filters.statusCurrent"),
      })),
    },
  ];

  return (
    <ScreenShell
      toolbar={
        <>
          <FilterMenu facets={facets} />
          <FilterInput
            value={query}
            onChange={setQuery}
            placeholder={t("filters.placeholderMarketplaces")}
          />
        </>
      }
      actions={
        <>
          <Button
            type="button"
            variant="secondary"
            disabled={staleTotal === 0 || busy || updatingAll}
            onClick={() => void runUpdate()}
          >
            {updatingAll ? (
              <RiLoader4Line className="size-4 animate-spin" aria-hidden="true" />
            ) : (
              <RiArrowUpCircleLine className="size-4" aria-hidden="true" />
            )}
            {t("marketplaces.updateAll", { count: staleTotal })}
          </Button>
          <Button
            ref={addButtonRef}
            type="button"
            variant="primary"
            disabled={busy}
            onClick={() => setAddOpen(true)}
          >
            <RiAddLine className="size-4" aria-hidden="true" />
            {t("marketplaces.add")}
          </Button>
        </>
      }
    >
      <SidecarLayout
        open={open}
        label={t("marketplaces.detail.paneLabel")}
        onClose={closeDetail}
        detail={<MarketplaceDetail marketplaceName={pane.id} onRemoved={closeDetail} />}
      >
        <div className={screenColumn}>
          {filteredMarketplaces.length === 0 ? (
            <EmptyState
              icon={marketplaces.length === 0 ? RiStoreLine : RiFileDamageLine}
              title={marketplaces.length === 0 ? t("marketplaces.empty") : t("filters.noResults")}
              body={
                marketplaces.length === 0 ? t("marketplaces.emptyBody") : t("filters.noResultsBody")
              }
            />
          ) : (
            <div className={contentFrame}>
              <ScrollArea className={cn("min-h-0", stickyHeaderScrollArea)}>
                <Table role="grid" className={framedTable} containerClassName="overflow-x-visible">
                  <TableHeader className={stickyTableHeader}>
                    <TableRow>
                      <TableHead className={cn("truncate", columns.name)}>
                        {t("marketplaces.columnName")}
                      </TableHead>
                      <TableHead className={cn("truncate", columns.source)}>
                        {t("marketplaces.columnSource")}
                      </TableHead>
                      <TableHead className={cn("text-right", columns.installed)}>
                        {t("marketplaces.columnInstalled")}
                      </TableHead>
                      <TableHead className={cn("text-right", columns.available)}>
                        {t("marketplaces.columnAvailable")}
                      </TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody ref={bodyRef}>
                    {filteredMarketplaces.map((marketplace) => {
                      const state = updateReport?.marketplaces[marketplace.name];
                      const installedCount = plugins.filter(
                        (plugin) => plugin.marketplaceName === marketplace.name,
                      ).length;
                      const availableCount = new Set(
                        availablePlugins
                          .filter((plugin) => plugin.marketplaceName === marketplace.name)
                          .map((plugin) => plugin.pluginId),
                      ).size;
                      return (
                        <TableRow
                          key={marketplace.name}
                          {...rowProps(marketplace.name)}
                          className={cn("hover:bg-bg-subtle", insetFocusRing)}
                        >
                          <TableCell className="font-medium text-text">
                            <span className="flex min-w-0 items-center gap-2">
                              <span className="truncate">{marketplace.name}</span>
                              {staleMarketplaces.has(marketplace.name) ? (
                                <StaleBadge>{describeUpdateState(state, t)}</StaleBadge>
                              ) : null}
                            </span>
                          </TableCell>
                          <TableCell
                            className={cn("truncate text-text-muted", columns.source)}
                            title={marketplace.displaySource}
                          >
                            {marketplace.displaySource}
                          </TableCell>
                          <TableCell className={cn("text-right text-text-muted", columns.installed)}>
                            {installedCount}
                          </TableCell>
                          <TableCell className={cn("text-right text-text-muted", columns.available)}>
                            {availableCount}
                          </TableCell>
                        </TableRow>
                      );
                    })}
                  </TableBody>
                </Table>
              </ScrollArea>
            </div>
          )}
        </div>
      </SidecarLayout>

      <AddMarketplaceDialog open={addOpen} onOpenChange={setAddOpen} />
    </ScreenShell>
  );
}
