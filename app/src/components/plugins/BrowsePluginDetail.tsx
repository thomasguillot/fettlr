import {
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@fettlr/design-system";
import { RiPuzzleLine, RiArrowDownSLine } from "@remixicon/react";
import { useTranslation } from "react-i18next";

import type { UiPlugin, UiProject, UiScope } from "../../bindings";
import { DetailEmpty, DetailHeader, DetailRow } from "../screen";
import { projectLabel } from "../projects";

export function BrowsePluginDetail({
  entry,
  projects,
  busy,
  onInstall,
}: {
  entry: UiPlugin | null;
  projects: UiProject[];
  busy: boolean;
  onInstall: (pluginId: string, scope: UiScope) => void;
}) {
  const { t } = useTranslation();

  if (!entry) {
    return (
      <DetailEmpty icon={RiPuzzleLine} message={t("plugins.detail.empty")} />
    );
  }

  return (
    <div className="flex flex-1 flex-col gap-6 px-6 pb-6">
      <DetailHeader title={entry.name} />

      <p className="font-mono text-sm text-text-muted">{entry.pluginId}</p>

      <dl className="flex flex-col gap-4">
        <DetailRow label={t("plugins.detail.description")}>
          {entry.description ?? (
            <span className="text-text-muted">{t("plugins.detail.noDescription")}</span>
          )}
        </DetailRow>
        <DetailRow label={t("plugins.detail.marketplace")}>{entry.marketplaceName}</DetailRow>
        <DetailRow label={t("plugins.detail.version")}>{entry.version}</DetailRow>
      </dl>

      <div className="mt-auto grid grid-cols-2 gap-2">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              type="button"
              variant="primary"
              disabled={busy}
              aria-label={t("plugins.browse.installAria", { name: entry.name })}
            >
              {t("plugins.browse.install")}
              <RiArrowDownSLine className="size-4" aria-hidden="true" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent>
            <DropdownMenuLabel>{t("plugins.browse.installFor")}</DropdownMenuLabel>
            <DropdownMenuItem onSelect={() => onInstall(entry.pluginId, { kind: "user" })}>
              {t("skills.scopeUser")}
            </DropdownMenuItem>
            {projects.length > 0 ? (
              <>
                <DropdownMenuSeparator />
                {projects.map((project) => (
                  <DropdownMenuItem
                    key={project.path}
                    onSelect={() =>
                      onInstall(entry.pluginId, { kind: "project", path: project.path })
                    }
                  >
                    {projectLabel(project.path)}
                  </DropdownMenuItem>
                ))}
              </>
            ) : null}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </div>
  );
}
