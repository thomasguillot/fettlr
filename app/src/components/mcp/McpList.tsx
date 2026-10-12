import {
  Button,
  ScrollArea,
  Switch,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
  cn,
  insetFocusRing,
} from "@fettlr/design-system";
import { RiAddLine, RiFileDamageLine, RiServerLine } from "@remixicon/react";
import { useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { commands, type UiMcpOrigin, type UiMcpServer } from "../../bindings";
import { useDetailPane, useFettlr } from "../../store";
import { OriginTag, UpdateStatusBadge, originKindLabel } from "../badges";
import { FilterInput } from "../FilterInput";
import { ANY, FilterMenu, type FilterFacet } from "../FilterMenu";
import { ScreenShell, SidecarLayout, contentFrame, framedTable, screenColumn, stickyHeaderScrollArea, stickyTableHeader } from "../screen";
import { useRevealReset } from "../useRevealReset";
import { useSidecarRows } from "../useSidecarRows";
import { matchesText, mcpSearchValue } from "../searchText";
import { McpDetail } from "./McpDetail";
import { McpFormDialog } from "./McpFormDialog";
import { EmptyState } from "../EmptyState";

const EMPTY_SERVERS: UiMcpServer[] = [];

const ORIGIN_KINDS = ["userScope", "localScope", "projectFile", "plugin"] as const satisfies readonly UiMcpOrigin["kind"][];

type ScopeFilter = typeof ANY | UiMcpOrigin["kind"];
type EnabledFilter = typeof ANY | "true" | "false";

export function McpList() {
  const { t } = useTranslation();
  const snapshot = useFettlr((state) => state.snapshot);
  const dispatch = useFettlr((state) => state.dispatch);
  const busy = useFettlr((state) => state.busy);
  const pane = useDetailPane("mcp");
  const reveal = useFettlr((state) => state.reveal);
  const [formTarget, setFormTarget] = useState<"add" | string | null>(null);
  const [scopeFilter, setScopeFilter] = useState<ScopeFilter>(ANY);
  const [transportFilter, setTransportFilter] = useState<string>(ANY);
  const [enabledFilter, setEnabledFilter] = useState<EnabledFilter>(ANY);
  const [query, setQuery] = useState("");
  const addButtonRef = useRef<HTMLButtonElement>(null);

  const servers = snapshot?.mcpServers ?? EMPTY_SERVERS;
  const updateReport = snapshot?.updateReport ?? null;

  const scopeOptions = useMemo(() => {
    const kinds = new Set<UiMcpOrigin["kind"]>();
    for (const server of servers) kinds.add(server.origin.kind);
    return ORIGIN_KINDS.filter((kind) => kinds.has(kind));
  }, [servers]);

  const transportOptions = useMemo(() => {
    const transports = new Set<string>();
    for (const server of servers) transports.add(server.transport);
    return Array.from(transports).sort((a, b) => a.localeCompare(b));
  }, [servers]);

  const enabledOptions = useMemo(() => {
    const values = new Set<boolean>();
    for (const server of servers) values.add(server.isEnabled);
    return [true, false].filter((value) => values.has(value));
  }, [servers]);

  const filteredServers = useMemo(
    () =>
      servers
        .filter(
          (server) =>
            (scopeFilter === ANY || server.origin.kind === scopeFilter) &&
            (transportFilter === ANY || server.transport === transportFilter) &&
            (enabledFilter === ANY || String(server.isEnabled) === enabledFilter) &&
            matchesText(mcpSearchValue(server), query),
        )
        .sort((a, b) => a.name.localeCompare(b.name)),
    [servers, scopeFilter, transportFilter, enabledFilter, query],
  );

  useRevealReset(
    "mcp",
    filteredServers.map((server) => server.id),
    () => {
      setScopeFilter(ANY);
      setTransportFilter(ANY);
      setEnabledFilter(ANY);
      setQuery("");
    },
  );

  const { bodyRef, open, closeDetail, rowProps } = useSidecarRows(
    pane,
    filteredServers.map((server) => server.id),
    { fallbackFocus: () => addButtonRef.current?.focus() },
  );

  const columns = {
    name: "w-[26%]",
    transport: "w-[14%]",
    origin: "w-[25%]",
    enabled: "w-[11%]",
    status: "w-[24%]",
  };

  const facets: FilterFacet[] = [
    {
      id: "scope",
      label: t("mcp.columnOrigin"),
      anyLabel: t("filters.anyScope"),
      value: scopeFilter,
      onChange: (value) => setScopeFilter(value as ScopeFilter),
      options: scopeOptions.map((kind) => ({ value: kind, label: originKindLabel(kind, t) })),
    },
    {
      id: "transport",
      label: t("mcp.columnTransport"),
      anyLabel: t("filters.anyTransport"),
      value: transportFilter,
      onChange: setTransportFilter,
      options: transportOptions.map((transport) => ({ value: transport, label: transport })),
    },
    {
      id: "enabled",
      label: t("mcp.columnEnabled"),
      anyLabel: t("filters.anyState"),
      value: enabledFilter,
      onChange: (value) => setEnabledFilter(value as EnabledFilter),
      options: enabledOptions.map((value) => ({
        value: String(value),
        label: value ? t("common.enabled") : t("common.disabled"),
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
            placeholder={t("filters.placeholderMcp")}
          />
        </>
      }
      actions={
        <Button
          ref={addButtonRef}
          type="button"
          variant="primary"
          disabled={busy}
          onClick={() => setFormTarget("add")}
        >
          <RiAddLine className="size-4" aria-hidden="true" />
          {t("mcp.addButton")}
        </Button>
      }
    >
      <SidecarLayout
        open={open}
        label={t("mcp.detail.paneLabel")}
        onClose={closeDetail}
        detail={
          <McpDetail
            serverId={pane.id}
            open={open}
            onEdit={(id) => setFormTarget(id)}
            onRemoved={closeDetail}
          />
        }
      >
        <div className={screenColumn}>
          {filteredServers.length === 0 ? (
            <EmptyState
              icon={servers.length === 0 ? RiServerLine : RiFileDamageLine}
              title={servers.length === 0 ? t("mcp.empty") : t("filters.noResults")}
              body={servers.length === 0 ? t("mcp.emptyBody") : t("filters.noResultsBody")}
            />
          ) : (
            <div className={contentFrame}>
              <ScrollArea className={cn("min-h-0", stickyHeaderScrollArea)}>
                <Table role="grid" className={framedTable} containerClassName="overflow-x-visible">
                  <TableHeader className={stickyTableHeader}>
                    <TableRow>
                      <TableHead className={cn("truncate", columns.name)}>
                        {t("mcp.columnName")}
                      </TableHead>
                      <TableHead className={cn("truncate", columns.transport)}>
                        {t("mcp.columnTransport")}
                      </TableHead>
                      <TableHead className={cn("truncate", columns.origin)}>
                        {t("mcp.columnOrigin")}
                      </TableHead>
                      <TableHead className={cn("truncate", columns.enabled)}>
                        {t("mcp.columnEnabled")}
                      </TableHead>
                      <TableHead className={cn("truncate", columns.status)}>
                        {t("mcp.columnStatus")}
                      </TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody ref={bodyRef}>
                    {filteredServers.map((server) => {
                      const state = updateReport?.mcpServers[server.id];
                      const manageable = server.origin.kind !== "plugin";
                      return (
                        <TableRow
                          key={server.id}
                          {...rowProps(server.id)}
                          className={cn("hover:bg-bg-subtle", insetFocusRing)}
                        >
                          <TableCell
                            className={cn(
                              "truncate font-medium",
                              server.isEnabled ? "text-text" : "text-text-muted",
                            )}
                          >
                            {server.name}
                          </TableCell>
                          <TableCell className={cn("truncate text-text-muted", columns.transport)}>
                            {server.transport}
                          </TableCell>
                          <TableCell className={columns.origin}>
                            <OriginTag origin={server.origin} truncate />
                          </TableCell>
                          <TableCell onClick={(event) => event.stopPropagation()}>
                            <Switch
                              checked={server.isEnabled}
                              disabled={busy || !manageable}
                              onCheckedChange={(checked) =>
                                void dispatch(commands.setMcpServerEnabled(server.id, checked))
                              }
                              aria-label={t("mcp.enableAria", { name: server.name })}
                            />
                          </TableCell>
                          <TableCell className="overflow-hidden">
                            <UpdateStatusBadge state={state} pending="updateAvailable" tracksLatest />
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

      <McpFormDialog
        open={formTarget !== null}
        serverId={formTarget && formTarget !== "add" ? formTarget : null}
        onOpenChange={(nextOpen) => {
          if (!nextOpen) setFormTarget(null);
        }}
        onSaved={(id) => {
          setFormTarget(null);
          pane.select(id);
          reveal("mcp", id);
        }}
      />
    </ScreenShell>
  );
}
