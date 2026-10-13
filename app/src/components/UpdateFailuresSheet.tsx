import {
  Button,
  Sheet,
  SheetBody,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@fettlr/design-system";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { type UiUpdateFailure } from "../bindings";
import { type UpdateRun, useFettlr } from "../store";

export function failureKey(kind: UiUpdateFailure["kind"]): string {
  switch (kind) {
    case "marketplace":
      return "updateAll.failureMarketplace";
    case "plugin":
      return "updateAll.failurePlugin";
    case "skill":
      return "updateAll.failureSkill";
  }
}

export function UpdateFailuresSheet() {
  const { t } = useTranslation();
  const updateRun = useFettlr((state) => state.updateRun);
  const dismissUpdateRun = useFettlr((state) => state.dismissUpdateRun);
  const [shown, setShown] = useState<UpdateRun | null>(null);
  if (updateRun?.phase === "finished" && updateRun !== shown) setShown(updateRun);
  const attempted = shown?.attempted ?? 0;
  const failures = shown?.failures ?? [];

  return (
    <Sheet
      open={updateRun?.phase === "finished"}
      onOpenChange={(next) => {
        if (!next) dismissUpdateRun();
      }}
    >
      <SheetContent closeLabel={t("common.close")}>
        <SheetHeader>
          <SheetTitle>{t("updateAll.failuresTitle")}</SheetTitle>
          <SheetDescription>
            {t("updateAll.summary", { done: attempted - failures.length, total: attempted })}
          </SheetDescription>
        </SheetHeader>

        <SheetBody>
          <ul className="flex flex-col gap-1.5 rounded-md border border-border bg-bg-subtle p-2.5 text-xs text-text-muted">
            {failures.map((failure, index) => (
              <li key={`${index}:${failure.kind}:${failure.name}`}>
                {t(failureKey(failure.kind), { name: failure.name, error: failure.error })}
              </li>
            ))}
          </ul>
        </SheetBody>

        <SheetFooter>
          <Button type="button" variant="secondary" onClick={dismissUpdateRun}>
            {t("updateAll.done")}
          </Button>
        </SheetFooter>
      </SheetContent>
    </Sheet>
  );
}
