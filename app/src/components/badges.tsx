import { Badge, cn } from "@fettlr/design-system";
import { RiGitBranchLine, RiHardDrive2Line, RiPuzzleLine } from "@remixicon/react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

import type { UiMcpOrigin, UiProvenance, UiScope, UiUpdateState } from "../bindings";
import { describeUpdateState } from "../updateState";
import { projectLabel, scopeLabel } from "./projects";

export function StaleBadge({ children }: { children: ReactNode }) {
  return (
    <Badge tone="blue" variant="soft" className="max-w-full">
      <span className="min-w-0 truncate">{children}</span>
    </Badge>
  );
}

export function ScopeBadge({ scope }: { scope: UiScope }) {
  const { t } = useTranslation();
  return (
    <Badge variant="outline" className="max-w-full shrink-0">
      <span className="min-w-0 truncate">{scopeLabel(scope, t)}</span>
    </Badge>
  );
}

export function ProvenanceBadge({ provenance }: { provenance: UiProvenance }) {
  const { t } = useTranslation();
  const Icon =
    provenance.kind === "local" ? RiHardDrive2Line : provenance.kind === "gitClone" ? RiGitBranchLine : RiPuzzleLine;
  const label =
    provenance.kind === "local"
      ? t("skills.provenanceLocal")
      : provenance.kind === "gitClone"
        ? t("skills.provenanceGit")
        : t("skills.provenancePlugin");
  return (
    <Badge tone="neutral" variant="soft" className="shrink-0">
      <Icon aria-hidden="true" />
      {label}
    </Badge>
  );
}

export function ModelBadge({ model }: { model: string | null }) {
  const { t } = useTranslation();
  if (!model || model === "inherit") {
    return <span className="truncate text-sm text-text-muted">{t("subagents.modelInherit")}</span>;
  }
  return (
    <Badge variant="outline" className="max-w-full shrink-0">
      <span className="min-w-0 truncate">{model}</span>
    </Badge>
  );
}

export function originKindLabel(kind: UiMcpOrigin["kind"], t: (key: string) => string): string {
  switch (kind) {
    case "userScope":
      return t("mcp.originUser");
    case "localScope":
      return t("mcp.originLocal");
    case "projectFile":
      return t("mcp.originProjectFile");
    case "plugin":
      return t("mcp.originPlugin");
  }
}

export function OriginTag({ origin, truncate = false }: { origin: UiMcpOrigin; truncate?: boolean }) {
  const { t } = useTranslation();
  const kindLabel = originKindLabel(origin.kind, t);
  const source =
    origin.kind === "localScope" || origin.kind === "projectFile"
      ? projectLabel(origin.project)
      : origin.kind === "plugin"
        ? origin.pluginId
        : null;
  return (
    <div className={cn("flex items-center gap-1.5", truncate ? "min-w-0" : "flex-wrap")}>
      <Badge variant="outline" className="shrink-0">
        {kindLabel}
      </Badge>
      {source ? (
        truncate ? (
          <span className="min-w-0 flex-1 truncate text-xs text-text-muted" title={source}>
            {source}
          </span>
        ) : (
          <Badge tone="neutral" variant="soft" className="shrink-0">
            {source}
          </Badge>
        )
      ) : null}
    </div>
  );
}

export function UpdateStatusBadge({
  state,
  pending,
  stale = true,
  tracksLatest = false,
}: {
  state: UiUpdateState | undefined;
  pending: "behind" | "updateAvailable";
  stale?: boolean;
  tracksLatest?: boolean;
}) {
  const { t } = useTranslation();
  if (state?.kind === pending) {
    return stale ? (
      <StaleBadge>{describeUpdateState(state, t)}</StaleBadge>
    ) : (
      <Badge tone="neutral" variant="soft">
        {t("plugins.detail.unknownVersion")}
      </Badge>
    );
  }
  if (state?.kind === "upToDate") {
    return (
      <Badge tone="success" variant="soft">
        {t("common.upToDate")}
      </Badge>
    );
  }
  if (tracksLatest && state?.kind === "tracksLatest") {
    return (
      <Badge tone="neutral" variant="soft">
        {t("mcp.tracksLatest")}
      </Badge>
    );
  }
  return (
    <Badge tone="neutral" variant="soft">
      {t("common.unknownStatus")}
    </Badge>
  );
}
