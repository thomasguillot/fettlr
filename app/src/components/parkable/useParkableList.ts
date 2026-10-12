import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";

import type { UiPlugin, UiProject, UiProvenance, UiScope } from "../../bindings";
import { useDetailPane } from "../../store";
import { ANY, type FilterFacet, type FilterOption } from "../FilterMenu";
import { projectLabel } from "../projects";
import { useRevealReset } from "../useRevealReset";
import { matchesText } from "../searchText";
import { itemKey, parseItemKey, type Selection } from "./selection";

type ScopeFilter = typeof ANY | "user" | `project:${string}`;

export type ParkableActive = { id: string; scope: UiScope; provenance: UiProvenance };
export type ParkableParked = { id: string; name: string; scopeKey: string; originalPath: string };
export type SourceKind = { kind: UiProvenance["kind"]; labelKey: string };

const EMPTY_PROJECTS: UiProject[] = [];
const EMPTY_PLUGINS: UiPlugin[] = [];

function matchesSource(provenance: UiProvenance, filter: string): boolean {
  if (filter === ANY) return true;
  if (!filter.startsWith("plugin:")) return provenance.kind === filter;
  return provenance.kind === "plugin" && provenance.pluginId === filter.slice("plugin:".length);
}

function matchesScope(scope: UiScope, filter: ScopeFilter): boolean {
  if (filter === ANY) return true;
  if (filter === "user") return scope.kind === "user";
  return (scope.kind === "project" || scope.kind === "local") && scope.path === filter.slice("project:".length);
}

// Parked items only carry an opaque scope_key ("user" or a hashed project folder name), not the
// UiScope the active list uses, so a project filter matches on the original path prefix instead.
function matchesParkedScope(parked: ParkableParked, filter: ScopeFilter): boolean {
  if (filter === ANY) return true;
  if (filter === "user") return parked.scopeKey === "user";
  const projectPath = filter.slice("project:".length);
  return parked.scopeKey !== "user" && parked.originalPath.startsWith(`${projectPath}/`);
}

export function useParkableList<A extends ParkableActive, P extends ParkableParked, K extends string>({
  section,
  kind,
  items,
  parked,
  projects = EMPTY_PROJECTS,
  plugins = EMPTY_PLUGINS,
  sourceKinds,
  scopeColumnLabel,
  sourceColumnLabel,
  activeSearchValue,
  activeSortKey,
  parkedSearchValue,
}: {
  section: "skills" | "subagents";
  kind: K;
  items: A[];
  parked: P[];
  projects?: UiProject[];
  plugins?: UiPlugin[];
  sourceKinds: readonly SourceKind[];
  scopeColumnLabel: string;
  sourceColumnLabel: string;
  activeSearchValue: (item: A) => string;
  activeSortKey: (item: A) => string;
  parkedSearchValue: (parked: P) => string;
}) {
  const { t } = useTranslation();
  const pane = useDetailPane(section);
  const [scopeFilter, setScopeFilter] = useState<ScopeFilter>(ANY);
  const [sourceFilter, setSourceFilter] = useState<string>(ANY);
  const [query, setQuery] = useState("");
  const listRef = useRef<HTMLUListElement>(null);
  const optionRefs = useRef(new Map<string, HTMLLIElement>());

  // Only projects that actually hold an item, so the filter never offers a choice that empties the
  // list. Parked items carry an opaque scope_key, so they are matched back to a known project by
  // their original path instead.
  const projectOptions = useMemo(() => {
    const paths = new Set<string>();
    for (const item of items) {
      if (item.scope.kind === "project" || item.scope.kind === "local") paths.add(item.scope.path);
    }
    const known = projects.map((project) => project.path);
    for (const entry of parked) {
      if (entry.scopeKey === "user") continue;
      const owner = known.find((path) => entry.originalPath.startsWith(`${path}/`));
      if (owner) paths.add(owner);
    }
    return Array.from(paths).sort((a, b) => projectLabel(a).localeCompare(projectLabel(b)));
  }, [items, parked, projects]);

  const sourceOptions = useMemo<FilterOption[]>(() => {
    const present = new Set(items.map((item) => item.provenance.kind));
    const pluginIds = new Set(
      items.flatMap((item) =>
        item.provenance.kind === "plugin" ? [item.provenance.pluginId] : [],
      ),
    );
    const names = new Map(plugins.map((plugin) => [plugin.pluginId, plugin.name]));
    const options: FilterOption[] = sourceKinds
      .filter((source) => present.has(source.kind))
      .map((source) => ({ value: source.kind, label: t(source.labelKey) }));
    return options.concat(
      Array.from(pluginIds)
        .map((id) => ({ value: `plugin:${id}`, label: names.get(id) ?? id }))
        .sort((a, b) => a.label.localeCompare(b.label)),
    );
  }, [items, plugins, sourceKinds, t]);

  const filteredItems = useMemo(
    () =>
      items
        .filter(
          (item) =>
            matchesScope(item.scope, scopeFilter) &&
            matchesSource(item.provenance, sourceFilter) &&
            matchesText(activeSearchValue(item), query),
        )
        .sort((a, b) => activeSortKey(a).localeCompare(activeSortKey(b))),
    [items, scopeFilter, sourceFilter, query, activeSearchValue, activeSortKey],
  );

  // A parked item has no provenance to match, so any source filter excludes the whole group.
  const filteredParked = useMemo(
    () =>
      sourceFilter !== ANY
        ? []
        : parked
            .filter(
              (entry) =>
                matchesParkedScope(entry, scopeFilter) && matchesText(parkedSearchValue(entry), query),
            )
            .sort((a, b) => a.name.localeCompare(b.name)),
    [parked, scopeFilter, sourceFilter, query, parkedSearchValue],
  );

  const flatOptions = useMemo<Selection<K>[]>(
    () => [
      ...filteredItems.map((item) => ({ kind, id: item.id })),
      ...filteredParked.map((entry) => ({ kind: "parked" as const, id: entry.id })),
    ],
    [filteredItems, filteredParked, kind],
  );

  useRevealReset(
    section,
    flatOptions.map((option) => itemKey(option.kind, option.id)),
    () => {
      setScopeFilter(ANY);
      setSourceFilter(ANY);
      setQuery("");
    },
  );

  const storedSelection = parseItemKey(pane.id, [kind]);
  const exists =
    storedSelection !== null &&
    (storedSelection.kind === "parked"
      ? filteredParked.some((entry) => entry.id === storedSelection.id)
      : filteredItems.some((item) => item.id === storedSelection.id));
  const open = pane.open && exists;
  const selection = open ? storedSelection : null;

  const activeIndex = useMemo(
    () =>
      selection
        ? flatOptions.findIndex((option) => option.kind === selection.kind && option.id === selection.id)
        : -1,
    [flatOptions, selection],
  );

  const optionDomId = (option: Selection<K>) => `${section}-option-${option.kind}-${option.id}`;
  const activeDomId = activeIndex >= 0 ? optionDomId(flatOptions[activeIndex]) : undefined;

  useEffect(() => {
    if (!activeDomId) return;
    optionRefs.current.get(activeDomId)?.scrollIntoView({ block: "nearest" });
  }, [activeDomId]);

  function registerOptionRef(id: string, el: HTMLLIElement | null) {
    if (el) optionRefs.current.set(id, el);
    else optionRefs.current.delete(id);
  }

  function selectAt(index: number) {
    if (index < 0 || index >= flatOptions.length) return;
    pane.select(itemKey(flatOptions[index].kind, flatOptions[index].id));
  }

  function selectOption(option: Selection<K>) {
    pane.select(itemKey(option.kind, option.id));
    listRef.current?.focus();
  }

  function closeDetail() {
    listRef.current?.focus();
    pane.close();
  }

  function handleListKeyDown(event: KeyboardEvent<HTMLUListElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      pane.close();
      return;
    }
    if (flatOptions.length === 0) return;
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        selectAt(activeIndex < 0 ? 0 : Math.min(activeIndex + 1, flatOptions.length - 1));
        break;
      case "ArrowUp":
        event.preventDefault();
        selectAt(activeIndex < 0 ? flatOptions.length - 1 : Math.max(activeIndex - 1, 0));
        break;
      case "Home":
        event.preventDefault();
        selectAt(0);
        break;
      case "End":
        event.preventDefault();
        selectAt(flatOptions.length - 1);
        break;
      default:
        break;
    }
  }

  function optionProps(optionKind: K | "parked", id: string) {
    const option = { kind: optionKind, id };
    const optionId = optionDomId(option);
    return {
      selected: selection?.kind === optionKind && selection.id === id,
      optionId,
      optionRef: (el: HTMLLIElement | null) => registerOptionRef(optionId, el),
      onSelect: () => selectOption(option),
    };
  }

  const facets: FilterFacet[] = [
    {
      id: "scope",
      label: scopeColumnLabel,
      anyLabel: t("filters.anyScope"),
      value: scopeFilter,
      onChange: (value) => setScopeFilter(value as ScopeFilter),
      options: [
        { value: "user", label: t("skills.scopeUser") },
        ...projectOptions.map((path) => ({ value: `project:${path}`, label: projectLabel(path) })),
      ],
    },
    {
      id: "source",
      label: sourceColumnLabel,
      anyLabel: t("filters.anySource"),
      value: sourceFilter,
      onChange: setSourceFilter,
      options: sourceOptions,
    },
  ];

  return {
    facets,
    query,
    setQuery,
    filteredItems,
    filteredParked,
    storedSelection,
    open,
    listRef,
    activeDomId,
    handleListKeyDown,
    optionProps,
    closeDetail,
    noResults: filteredItems.length === 0 && filteredParked.length === 0,
    nothingExists: items.length === 0 && parked.length === 0,
  };
}
