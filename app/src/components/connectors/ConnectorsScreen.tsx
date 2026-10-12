import {
  Badge,
  Button,
  Card,
  ScrollArea,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
  cn,
  focusRing,
} from "@fettlr/design-system";
import { RiCloudLine, RiLoader4Line, RiRefreshLine } from "@remixicon/react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { commands, type UiConnector, type UiConnectorState } from "../../bindings";
import { useDetailPane, useFettlr } from "../../store";
import {
  DetailEmpty,
  DetailHeader,
  DetailRow,
  ScreenShell,
  SidecarLayout,
  codeChip,
  contentFrame,
  formatTimestamp,
  framedTable,
  screenColumn,
  stickyHeaderScrollArea,
  stickyTableHeader,
} from "../screen";
import { useSidecarRows } from "../useSidecarRows";

const stateTone: Record<UiConnectorState, "success" | "danger" | "warning" | "neutral"> = {
  connected: "success",
  failed: "danger",
  pending: "warning",
  inactive: "neutral",
  unknown: "neutral",
};

const stateKey: Record<UiConnectorState, string> = {
  connected: "connectors.stateConnected",
  failed: "connectors.stateFailed",
  pending: "connectors.statePending",
  inactive: "connectors.stateInactive",
  unknown: "connectors.stateUnknown",
};

export function ConnectorsScreen() {
  const { t } = useTranslation();
  const dispatch = useFettlr((state) => state.dispatch);
  const busy = useFettlr((state) => state.busy);
  const connectors = useFettlr((state) => state.snapshot?.connectors ?? []);
  const checkedAt = useFettlr((state) => state.snapshot?.connectorsCheckedAt ?? null);
  const pane = useDetailPane("connectors");
  const [checking, setChecking] = useState(false);

  const check = async () => {
    setChecking(true);
    try {
      await dispatch(commands.refreshConnectors());
    } finally {
      setChecking(false);
    }
  };

  const checkButton = (variant: "primary" | "secondary") => (
    <Button type="button" variant={variant} disabled={busy || checking} onClick={() => void check()}>
      {checking ? (
        <RiLoader4Line className="size-4 animate-spin" aria-hidden="true" />
      ) : (
        <RiRefreshLine className="size-4" aria-hidden="true" />
      )}
      {checking ? t("connectors.checking") : t("connectors.check")}
    </Button>
  );

  const { bodyRef, open, selectedId, closeDetail, rowProps } = useSidecarRows(
    pane,
    connectors.map((connector) => connector.name),
  );

  return (
    <ScreenShell
      toolbar={
        checkedAt ? (
          <p className="truncate text-sm text-text-muted">
            {t("connectors.lastChecked", { time: formatTimestamp(checkedAt) })}
          </p>
        ) : undefined
      }
      actions={checkedAt ? checkButton("secondary") : undefined}
    >
      <SidecarLayout
        open={open}
        label={t("connectors.paneLabel")}
        onClose={closeDetail}
        detail={<ConnectorDetail connectorName={pane.id} />}
      >
        <div className={screenColumn}>
          {connectors.length === 0 ? (
            <div className="flex flex-1 flex-col">
              <Card>
                <div className="flex flex-col items-center gap-3 px-6 py-10 text-center">
                  <span
                    aria-hidden="true"
                    className="flex size-10 items-center justify-center rounded-full bg-accent-soft"
                  >
                    <RiCloudLine className="size-5 text-accent-text" />
                  </span>
                  <div className="flex flex-col items-center gap-1">
                    <h2 className="text-lg font-semibold tracking-tight text-text">
                      {t(checkedAt ? "connectors.emptyNone" : "connectors.empty")}
                    </h2>
                    <p className="max-w-prose text-sm text-text-muted">
                      {t(checkedAt ? "connectors.emptyNoneBody" : "connectors.emptyBody")}
                    </p>
                  </div>
                  <div className="mt-1">{checkButton("primary")}</div>
                </div>
              </Card>
            </div>
          ) : (
            <div className={contentFrame}>
              <ScrollArea className={cn("min-h-0", stickyHeaderScrollArea)}>
                <Table role="grid" className={framedTable} containerClassName="overflow-x-visible">
                  <TableHeader className={stickyTableHeader}>
                    <TableRow>
                      <TableHead className="w-1/4 truncate">{t("connectors.name")}</TableHead>
                      <TableHead className="truncate">{t("connectors.endpoint")}</TableHead>
                      <TableHead className="w-40 truncate">{t("connectors.status")}</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody ref={bodyRef}>
                    {connectors.map((connector) => (
                      <TableRow
                        key={connector.name}
                        {...rowProps(connector.name)}
                        data-state={undefined}
                        className={cn(
                          "hover:bg-bg-subtle",
                          selectedId === connector.name && "bg-surface-hover",
                          focusRing,
                        )}
                      >
                        <TableCell
                          className={cn(
                            "truncate font-medium",
                            // Inactive is the only state where Claude Code is not serving the
                            // connector. A failure is listed and wants the eye, not less of it.
                            connector.state === "inactive" ? "text-text-muted" : "text-text",
                          )}
                        >
                          {connector.label}
                        </TableCell>
                        <TableCell className="truncate text-text-muted">
                          {connector.target ?? "—"}
                        </TableCell>
                        <TableCell>
                          <Badge tone={stateTone[connector.state]} variant="soft">
                            {t(stateKey[connector.state])}
                          </Badge>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </ScrollArea>
            </div>
          )}
          <p className="mt-auto pt-6 text-xs text-text-muted">{t("connectors.footnote")}</p>
        </div>
      </SidecarLayout>
    </ScreenShell>
  );
}

function ConnectorDetail({ connectorName }: { connectorName: string | null }) {
  const { t } = useTranslation();
  const connectors = useFettlr((state) => state.snapshot?.connectors ?? []);
  const connector: UiConnector | null = connectorName
    ? (connectors.find((candidate) => candidate.name === connectorName) ?? null)
    : null;

  if (!connector) {
    return <DetailEmpty icon={RiCloudLine} message={t("connectors.detailTitle")} />;
  }

  return (
    <div className="flex flex-1 flex-col gap-6 px-6 pb-6">
      <DetailHeader title={connector.label} />
      <p className="text-sm text-text-muted">{t("connectors.detailBody")}</p>
      <dl className="flex flex-col gap-4">
        <DetailRow label={t("connectors.endpoint")}>
          {connector.target ? (
            <span className={cn(codeChip, "break-all")}>{connector.target}</span>
          ) : (
            <span className="text-text-muted">{t("connectors.noEndpoint")}</span>
          )}
        </DetailRow>
        <DetailRow label={t("connectors.status")}>
          <Badge tone={stateTone[connector.state]} variant="soft">
            {t(stateKey[connector.state])}
          </Badge>
          {/* The badge says nothing useful for a status Fettlr could not classify, so the
              sentence the CLI printed is kept for that case alone. */}
          {connector.state === "unknown" && connector.status ? (
            <span className="mt-1 block text-text-muted">{connector.status}</span>
          ) : null}
        </DetailRow>
      </dl>
    </div>
  );
}
