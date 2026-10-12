import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandShortcut,
  cn,
} from "@fettlr/design-system";
import {
  RiPuzzleLine,
  RiFolderLine,
  RiRobot2Line,
  RiServerLine,
  RiFileDamageLine,
  RiSparklingLine,
  RiStoreLine,
} from "@remixicon/react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type { UiMcpServer } from "../../bindings";
import { useFettlr } from "../../store";
import { itemKey } from "../parkable/selection";
import { projectLabel, scopeLabel } from "../projects";
import {
  marketplaceSearchValue,
  mcpSearchValue,
  parkedSearchValue,
  parkedSubagentSearchValue,
  pluginSearchValue,
  skillSearchValue,
  subagentSearchValue,
} from "../searchText";

function mcpTrailingLabel(server: UiMcpServer): string {
  if (server.origin.kind === "localScope" || server.origin.kind === "projectFile") {
    return projectLabel(server.origin.project);
  }
  if (server.origin.kind === "plugin") return server.origin.pluginId;
  return server.transport;
}

// cmdk's default filter scores subsequences, so a query like "fgjk" matches any item whose summary
// happens to contain those letters in order. Require a real substring instead, ranked by how early
// it appears.
function matchScore(value: string, search: string): number {
  const needle = search.trim().toLowerCase();
  if (needle === "") return 1;
  const index = value.toLowerCase().indexOf(needle);
  return index < 0 ? 0 : 1 / (1 + index);
}

export function SearchPalette() {
  const { t } = useTranslation();
  const panel = useFettlr((state) => state.panel);
  const setPanel = useFettlr((state) => state.setPanel);
  const snapshot = useFettlr((state) => state.snapshot);
  const select = useFettlr((state) => state.select);
  const closeDetail = useFettlr((state) => state.closeDetail);
  const navigateToItem = useFettlr((state) => state.navigateToItem);
  const [query, setQuery] = useState("");
  const listRef = useRef<HTMLDivElement>(null);

  const open = panel === "search";

  const [wasOpen, setWasOpen] = useState(open);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (!open) setQuery("");
  }

  const skills = useMemo(
    () => [...(snapshot?.skills ?? [])].sort((a, b) => (a.title ?? a.name).localeCompare(b.title ?? b.name)),
    [snapshot],
  );
  const parkedSkills = useMemo(
    () => [...(snapshot?.parkedSkills ?? [])].sort((a, b) => a.name.localeCompare(b.name)),
    [snapshot],
  );
  const subagents = useMemo(
    () => [...(snapshot?.subagents ?? [])].sort((a, b) => a.name.localeCompare(b.name)),
    [snapshot],
  );
  const parkedSubagents = useMemo(
    () => [...(snapshot?.parkedSubagents ?? [])].sort((a, b) => a.name.localeCompare(b.name)),
    [snapshot],
  );
  const plugins = useMemo(
    () => [...(snapshot?.plugins ?? [])].sort((a, b) => a.name.localeCompare(b.name)),
    [snapshot],
  );
  const marketplaces = useMemo(
    () => [...(snapshot?.marketplaces ?? [])].sort((a, b) => a.name.localeCompare(b.name)),
    [snapshot],
  );
  const mcpServers = useMemo(
    () => [...(snapshot?.mcpServers ?? [])].sort((a, b) => a.name.localeCompare(b.name)),
    [snapshot],
  );
  const projects = useMemo(
    () =>
      [...(snapshot?.projects ?? [])]
        .filter((project) => project.exists)
        .sort((a, b) => projectLabel(a.path).localeCompare(projectLabel(b.path))),
    [snapshot],
  );

  function close() {
    setPanel(null);
  }

  function choose(action: () => void) {
    action();
    close();
  }

  function chooseProject(path: string) {
    const inProject = skills.filter(
      (skill) =>
        (skill.scope.kind === "project" || skill.scope.kind === "local") && skill.scope.path === path,
    );
    if (inProject.length > 0) {
      navigateToItem("skills", itemKey("skill", inProject[0].id));
      return;
    }
    const scopedServers = mcpServers.filter(
      (server) =>
        (server.origin.kind === "localScope" || server.origin.kind === "projectFile") &&
        server.origin.project === path,
    );
    if (scopedServers.length > 0) {
      navigateToItem("mcp", scopedServers[0].id);
      return;
    }
    select("skills");
    closeDetail("skills");
  }

  const hasQuery = query.trim() !== "";

  // cmdk keeps the scroll position across result changes, which leaves a narrowed list pre-scrolled.
  useEffect(() => {
    if (listRef.current) listRef.current.scrollTop = 0;
  }, [query]);

  return (
    <CommandDialog
      open={open}
      onOpenChange={(next) => setPanel(next ? "search" : null)}
      title={t("search.dialogTitle")}
      description={t("search.dialogDescription")}
      filter={matchScore}
    >
      <CommandInput
        value={query}
        onValueChange={setQuery}
        placeholder={t("search.placeholder")}
        aria-label={t("search.placeholder")}
        rowClassName="pointer-events-auto rounded-xl border border-border bg-surface"
      />
      {/* Nothing here may carry a radius and a clip at once: WebKit composites a clipping box and
          applies the clip without the border-radius, squaring the corners. The card owns the
          radius and never clips; CommandList scrolls inside it and owns no radius. */}
      <div
        className={cn(
          "pointer-events-auto rounded-xl border border-border bg-surface",
          !hasQuery && "invisible",
        )}
      >
        <CommandList ref={listRef} className="h-72">
          {hasQuery ? (
            <>
              <CommandEmpty className="flex min-h-64 flex-col items-center justify-center gap-3 px-6">
                <span
                  aria-hidden="true"
                  className="flex size-10 items-center justify-center rounded-full bg-accent-soft"
                >
                  <RiFileDamageLine className="size-5 text-accent-text" />
                </span>
                <div className="flex flex-col items-center gap-1">
                  <p className="text-lg font-semibold tracking-tight text-text">
                    {t("search.empty")}
                  </p>
                  <p className="max-w-prose text-sm text-text-muted">
                    {t("search.emptyBody")}
                  </p>
                </div>
              </CommandEmpty>
              {(() => {
                const skillItems = (
                  <>
                    {skills.map((skill) => (
                      <CommandItem
                        key={`skill:${skill.id}`}
                        value={skillSearchValue(skill)}
                        onSelect={() =>
                          choose(() => navigateToItem("skills", itemKey("skill", skill.id)))
                        }
                      >
                        <RiSparklingLine aria-hidden="true" />
                        <span className="min-w-0 flex-1 truncate">{skill.title ?? skill.name}</span>
                        <CommandShortcut>{scopeLabel(skill.scope, t)}</CommandShortcut>
                      </CommandItem>
                    ))}
                    {parkedSkills.map((parked) => (
                      <CommandItem
                        key={`parked:${parked.id}`}
                        value={parkedSearchValue(parked)}
                        onSelect={() =>
                          choose(() => navigateToItem("skills", itemKey("parked", parked.id)))
                        }
                      >
                        <RiSparklingLine aria-hidden="true" />
                        <span className="min-w-0 flex-1 truncate">{parked.name}</span>
                        <CommandShortcut>{t("skills.parkedBadge")}</CommandShortcut>
                      </CommandItem>
                    ))}
                  </>
                );

                const subagentItems = (
                  <>
                    {subagents.map((subagent) => (
                      <CommandItem
                        key={`subagent:${subagent.id}`}
                        value={subagentSearchValue(subagent)}
                        onSelect={() =>
                          choose(() =>
                            navigateToItem(
                              "subagents",
                              itemKey("subagent", subagent.id),
                            ),
                          )
                        }
                      >
                        <RiRobot2Line aria-hidden="true" />
                        <span className="min-w-0 flex-1 truncate">{subagent.name}</span>
                        <CommandShortcut>{scopeLabel(subagent.scope, t)}</CommandShortcut>
                      </CommandItem>
                    ))}
                    {parkedSubagents.map((parked) => (
                      <CommandItem
                        key={`parked-subagent:${parked.id}`}
                        value={parkedSubagentSearchValue(parked)}
                        onSelect={() =>
                          choose(() =>
                            navigateToItem("subagents", itemKey("parked", parked.id)),
                          )
                        }
                      >
                        <RiRobot2Line aria-hidden="true" />
                        <span className="min-w-0 flex-1 truncate">{parked.name}</span>
                        <CommandShortcut>{t("subagents.parkedBadge")}</CommandShortcut>
                      </CommandItem>
                    ))}
                  </>
                );

                const pluginItems = (
                  <>
                    {plugins.map((plugin) => (
                      <CommandItem
                        key={plugin.id}
                        value={pluginSearchValue(plugin)}
                        onSelect={() => choose(() => navigateToItem("plugins", plugin.id))}
                      >
                        <RiPuzzleLine aria-hidden="true" />
                        <span className="min-w-0 flex-1 truncate">{plugin.name}</span>
                        <CommandShortcut>{plugin.marketplaceName}</CommandShortcut>
                      </CommandItem>
                    ))}
                  </>
                );

                const marketplaceItems = (
                  <>
                    {marketplaces.map((marketplace) => (
                      <CommandItem
                        key={marketplace.name}
                        value={marketplaceSearchValue(marketplace)}
                        onSelect={() => choose(() => navigateToItem("marketplaces", marketplace.name))}
                      >
                        <RiStoreLine aria-hidden="true" />
                        <span className="min-w-0 flex-1 truncate">{marketplace.name}</span>
                        <CommandShortcut>{marketplace.displaySource}</CommandShortcut>
                      </CommandItem>
                    ))}
                  </>
                );

                const mcpItems = (
                  <>
                    {mcpServers.map((server) => (
                      <CommandItem
                        key={server.id}
                        value={mcpSearchValue(server)}
                        onSelect={() => choose(() => navigateToItem("mcp", server.id))}
                      >
                        <RiServerLine aria-hidden="true" />
                        <span className="min-w-0 flex-1 truncate">{server.name}</span>
                        <CommandShortcut>{mcpTrailingLabel(server)}</CommandShortcut>
                      </CommandItem>
                    ))}
                  </>
                );

                const projectItems = (
                  <>
                    {projects.map((project) => (
                      <CommandItem
                        key={project.path}
                        value={project.path}
                        onSelect={() => choose(() => chooseProject(project.path))}
                      >
                        <RiFolderLine aria-hidden="true" />
                        <span className="min-w-0 flex-1 truncate">{projectLabel(project.path)}</span>
                        <CommandShortcut className="max-w-[45%] truncate">{project.path}</CommandShortcut>
                      </CommandItem>
                    ))}
                  </>
                );

                return (
                  <>
                    <CommandGroup heading={t("sections.skills")}>{skillItems}</CommandGroup>
                    <CommandGroup heading={t("sections.subagents")}>{subagentItems}</CommandGroup>
                    <CommandGroup heading={t("sections.plugins")}>{pluginItems}</CommandGroup>
                    <CommandGroup heading={t("sections.marketplaces")}>{marketplaceItems}</CommandGroup>
                    <CommandGroup heading={t("sections.mcp")}>{mcpItems}</CommandGroup>
                    <CommandGroup heading={t("search.groupProjects")}>{projectItems}</CommandGroup>
                  </>
                );
              })()}
            </>
          ) : null}
        </CommandList>
      </div>
    </CommandDialog>
  );
}
