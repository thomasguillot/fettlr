import type { UiProject, UiScope } from "../bindings";

type Translate = (key: string, options?: Record<string, unknown>) => string;

export function projectLabel(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  const base = trimmed.split("/").filter(Boolean).pop();
  return base ?? path;
}

export function scopeLabel(scope: UiScope, t: Translate): string {
  switch (scope.kind) {
    case "user":
      return t("skills.scopeUser");
    case "project":
      return projectLabel(scope.path);
    case "local":
      return `${projectLabel(scope.path)} · ${t("skills.scopeLocal")}`;
    case "other":
      return t("skills.scopeOther", { name: scope.name });
  }
}

export function parkedScopeLabel(
  parked: { scopeKey: string; originalPath: string },
  projects: UiProject[],
  t: (key: string) => string,
): string {
  if (parked.scopeKey === "user") return t("skills.scopeUser");
  const project = projects.find((candidate) => parked.originalPath.startsWith(`${candidate.path}/`));
  return project ? projectLabel(project.path) : t("skills.scopeProjectUnknown");
}
