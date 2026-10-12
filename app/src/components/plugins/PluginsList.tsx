import {
  ScrollArea,
  Switch,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  cn,
  insetFocusRing,
} from "@fettlr/design-system";
import { RiFileDamageLine, RiPuzzleLine } from "@remixicon/react";
import { useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { commands, type UiPlugin, type UiScope } from "../../bindings";
import { staleIds } from "../../sections";
import { useDetailPane, useFettlr } from "../../store";
import { describeUpdateState } from "../../updateState";
import { ScopeBadge, StaleBadge } from "../badges";
import { FilterInput } from "../FilterInput";
import { ANY, FilterMenu, type FilterFacet } from "../FilterMenu";
import { ScreenShell, SidecarLayout, contentFrame, framedTable, screenColumn, stickyHeaderScrollArea, stickyTableHeader } from "../screen";
import { useRevealReset } from "../useRevealReset";
import { useSidecarRows } from "../useSidecarRows";
import { matchesText, pluginSearchValue } from "../searchText";
import { projectLabel } from "../projects";
import { BrowseTab } from "./BrowseTab";
import { PluginDetail } from "./PluginDetail";
import { EmptyState } from "../EmptyState";

const EMPTY_PLUGINS: UiPlugin[] = [];

type ScopeFilter = typeof ANY | "user" | `project:${string}`;
type StatusFilter = typeof ANY | "stale" | "current";

function matchesScope(scope: UiScope, filter: ScopeFilter): boolean {
  if (filter === ANY) return true;
  if (filter === "user") return scope.kind === "user";
  return (scope.kind === "project" || scope.kind === "local") && scope.path === filter.slice("project:".length);
}

export function PluginsList() {
  const { t } = useTranslation();
  const snapshot = useFettlr((state) => state.snapshot);
  const dispatch = useFettlr((state) => state.dispatch);
  const busy = useFettlr((state) => state.busy);
  const pane = useDetailPane("plugins");
  const [tab, setTab] = useState<"installed" | "browse">("installed");
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState("");
  const [scopeFilter, setScopeFilter] = useState<ScopeFilter>(ANY);
  const [marketplaceFilter, setMarketplaceFilter] = useState<string>(ANY);
  const [statusFilter, setStatusFilter] = useState<StatusFilter>(ANY);
  const [browseMarketplace, setBrowseMarketplace] = useState<string>(ANY);
  const installedTabRef = useRef<HTMLButtonElement>(null);

  const plugins = snapshot?.plugins ?? EMPTY_PLUGINS;
  const updateReport = snapshot?.updateReport ?? null;
  const stalePlugins = useMemo(() => staleIds(updateReport, "plugins"), [updateReport]);

  const scopeOptions = useMemo(() => {
    let hasUser = false;
    const paths = new Set<string>();
    for (const plugin of plugins) {
      if (plugin.scope.kind === "user") hasUser = true;
      else if (plugin.scope.kind !== "other") paths.add(plugin.scope.path);
    }
    return {
      hasUser,
      projects: Array.from(paths).sort((a, b) => projectLabel(a).localeCompare(projectLabel(b))),
    };
  }, [plugins]);

  const marketplaceOptions = useMemo(() => {
    const names = new Set<string>();
    for (const plugin of plugins) names.add(plugin.marketplaceName);
    return Array.from(names).sort((a, b) => a.localeCompare(b));
  }, [plugins]);

  const browseMarketplaceOptions = useMemo(() => {
    const installed = new Set(plugins.map((plugin) => plugin.pluginId));
    const names = new Set<string>();
    for (const entry of snapshot?.availablePlugins ?? EMPTY_PLUGINS) {
      if (!installed.has(entry.pluginId)) names.add(entry.marketplaceName);
    }
    return Array.from(names).sort((a, b) => a.localeCompare(b));
  }, [snapshot, plugins]);

  const statusOptions = useMemo(() => {
    const values = new Set<"stale" | "current">();
    for (const plugin of plugins) values.add(stalePlugins.has(plugin.id) ? "stale" : "current");
    return (["stale", "current"] as const).filter((value) => values.has(value));
  }, [plugins, stalePlugins]);

  const filteredPlugins = useMemo(
    () =>
      plugins
        .filter(
          (plugin) =>
            matchesScope(plugin.scope, scopeFilter) &&
            (marketplaceFilter === ANY || plugin.marketplaceName === marketplaceFilter) &&
            (statusFilter === ANY ||
              (stalePlugins.has(plugin.id) ? "stale" : "current") === statusFilter) &&
            matchesText(pluginSearchValue(plugin), filter),
        )
        .sort((a, b) => a.name.localeCompare(b.name)),
    [plugins, scopeFilter, marketplaceFilter, statusFilter, stalePlugins, filter],
  );

  useRevealReset(
    "plugins",
    filteredPlugins.map((plugin) => plugin.id),
    () => {
      setFilter("");
      setScopeFilter(ANY);
      setMarketplaceFilter(ANY);
      setStatusFilter(ANY);
    },
  );

  const browsing = tab === "browse";
  const { bodyRef, open, closeDetail, rowProps } = useSidecarRows(
    pane,
    filteredPlugins.map((plugin) => plugin.id),
    { active: !browsing, fallbackFocus: () => installedTabRef.current?.focus() },
  );

  // Any fresh external landing on an open, installed plugin should pull the reader back to the
  // Installed tab: a newly-opened pane (false-to-true, including re-selecting the same plugin after
  // closing its sidecar) or a new id arriving while a pane from an earlier selection is still open.
  // A manual tab click never touches pane.id or pane.open, so it still isn't fought.
  const [routedPane, setRoutedPane] = useState({ id: pane.id, open: pane.open });
  if (pane.id !== routedPane.id || pane.open !== routedPane.open) {
    setRoutedPane({ id: pane.id, open: pane.open });
    if (pane.open && plugins.some((plugin) => plugin.id === pane.id)) setTab("installed");
  }

  const columns = {
    name: "w-[42%]",
    marketplace: "w-[22%]",
    version: "w-[11%]",
    scope: "w-[13%]",
    enabled: "w-[12%]",
  };

  const facets: FilterFacet[] = [
    {
      id: "scope",
      label: t("plugins.columnScope"),
      anyLabel: t("filters.anyScope"),
      value: scopeFilter,
      onChange: (value) => setScopeFilter(value as ScopeFilter),
      options: [
        ...(scopeOptions.hasUser ? [{ value: "user", label: t("skills.scopeUser") }] : []),
        ...scopeOptions.projects.map((path) => ({
          value: `project:${path}`,
          label: projectLabel(path),
        })),
      ],
    },
    {
      id: "marketplace",
      label: t("plugins.columnMarketplace"),
      anyLabel: t("filters.anyMarketplace"),
      value: marketplaceFilter,
      onChange: setMarketplaceFilter,
      options: marketplaceOptions.map((name) => ({ value: name, label: name })),
    },
    {
      id: "status",
      label: t("plugins.columnStatus"),
      anyLabel: t("filters.anyStatus"),
      value: statusFilter,
      onChange: (value) => setStatusFilter(value as StatusFilter),
      options: statusOptions.map((status) => ({
        value: status,
        label: status === "stale" ? t("filters.statusStale") : t("filters.statusCurrent"),
      })),
    },
  ];

  const browseFacets: FilterFacet[] = [
    {
      id: "marketplace",
      label: t("plugins.columnMarketplace"),
      anyLabel: t("filters.anyMarketplace"),
      value: browseMarketplace,
      onChange: setBrowseMarketplace,
      options: browseMarketplaceOptions.map((name) => ({ value: name, label: name })),
    },
  ];

  return (
    <Tabs
      value={tab}
      onValueChange={(value) => setTab(value as "installed" | "browse")}
      className="flex h-full min-h-0 flex-col"
    >
      <ScreenShell
        toolbar={
          browsing ? (
            <>
              <FilterMenu facets={browseFacets} />
              <FilterInput
                value={query}
                onChange={setQuery}
                placeholder={t("filters.placeholderPlugins")}
              />
            </>
          ) : (
            <>
              <FilterMenu facets={facets} />
              <FilterInput
                value={filter}
                onChange={setFilter}
                placeholder={t("filters.placeholderPlugins")}
              />
            </>
          )
        }
        actions={
          <TabsList variant="segmented">
            <TabsTrigger ref={installedTabRef} value="installed">
              {t("plugins.tabInstalled")}
            </TabsTrigger>
            <TabsTrigger value="browse">{t("plugins.tabBrowse")}</TabsTrigger>
          </TabsList>
        }
      >
        <TabsContent value="installed" className="mt-0 flex min-h-0 flex-1 flex-col">
          <SidecarLayout
            open={open}
            label={t("plugins.detail.paneLabel")}
            onClose={closeDetail}
            detail={<PluginDetail pluginId={pane.id} onRemoved={closeDetail} />}
          >
            <div className={screenColumn}>
              {filteredPlugins.length === 0 ? (
                <EmptyState
                  icon={plugins.length === 0 ? RiPuzzleLine : RiFileDamageLine}
                  title={
                    plugins.length === 0 ? t("plugins.installedEmpty") : t("filters.noResults")
                  }
                  body={
                    plugins.length === 0
                      ? t("plugins.installedEmptyBody")
                      : t("filters.noResultsBody")
                  }
                />
              ) : (
                <div className={contentFrame}>
                  <ScrollArea className={cn("min-h-0", stickyHeaderScrollArea)}>
                    <Table role="grid" className={framedTable} containerClassName="overflow-x-visible">
                      <TableHeader className={stickyTableHeader}>
                        <TableRow>
                          <TableHead className={cn("truncate", columns.name)}>
                            {t("plugins.columnName")}
                          </TableHead>
                          <TableHead className={cn("truncate", columns.marketplace)}>
                            {t("plugins.columnMarketplace")}
                          </TableHead>
                          <TableHead className={cn("truncate", columns.version)}>
                            {t("plugins.columnVersion")}
                          </TableHead>
                          <TableHead className={cn("truncate", columns.scope)}>
                            {t("plugins.columnScope")}
                          </TableHead>
                          <TableHead className={cn("truncate", columns.enabled)}>
                            {t("plugins.columnEnabled")}
                          </TableHead>
                        </TableRow>
                      </TableHeader>
                      <TableBody ref={bodyRef}>
                        {filteredPlugins.map((plugin) => {
                          const state = updateReport?.plugins[plugin.id];
                          return (
                            <TableRow
                              key={plugin.id}
                              {...rowProps(plugin.id)}
                              className={cn("hover:bg-bg-subtle", insetFocusRing)}
                            >
                              <TableCell className="font-medium text-text">
                                <span className="flex min-w-0 items-center gap-2">
                                  <span className="truncate">{plugin.name}</span>
                                  {stalePlugins.has(plugin.id) ? (
                                    <StaleBadge>{describeUpdateState(state, t)}</StaleBadge>
                                  ) : null}
                                </span>
                              </TableCell>
                              <TableCell className={cn("truncate text-text-muted", columns.marketplace)}>
                                {plugin.marketplaceName}
                              </TableCell>
                              <TableCell className={cn("truncate text-text-muted", columns.version)}>
                                {plugin.version}
                              </TableCell>
                              <TableCell className="overflow-hidden">
                                <ScopeBadge scope={plugin.scope} />
                              </TableCell>
                              <TableCell onClick={(event) => event.stopPropagation()}>
                                <Switch
                                  checked={plugin.enabled}
                                  disabled={busy || plugin.scope.kind === "other"}
                                  onCheckedChange={(checked) =>
                                    void dispatch(
                                      commands.setPluginEnabled(plugin.pluginId, checked, plugin.scope),
                                    )
                                  }
                                  aria-label={t("plugins.enableAria", { name: plugin.name })}
                                />
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
        </TabsContent>

        <TabsContent value="browse" className="mt-0 flex min-h-0 flex-1 flex-col">
          <div className="flex min-h-0 flex-1 flex-col">
            <BrowseTab query={query} marketplace={browseMarketplace} />
          </div>
        </TabsContent>
      </ScreenShell>
    </Tabs>
  );
}
