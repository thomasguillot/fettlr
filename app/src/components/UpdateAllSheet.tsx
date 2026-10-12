import {
  Button,
  Sheet,
  SheetBody,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
  cn,
} from "@fettlr/design-system";
import { RiArrowUpCircleLine, RiCheckboxCircleLine, RiLoader4Line, RiAlertLine } from "@remixicon/react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { commands, type UiUpdateFailure } from "../bindings";
import { useFettlr } from "../store";

type Phase = "idle" | "updating" | "refreshing" | "succeeded" | "partial" | "failed";

// Each phase mounts in place of the last while the reader is watching the run, so the swap gets a
// bridge rather than a cut. Reduced motion keeps the fade and drops the lift.
const phaseIn =
  "translate-y-0 opacity-100 transition-[opacity,translate] " +
  "duration-[var(--duration-base)] ease-[var(--ease-out)] " +
  "starting:translate-y-1 starting:opacity-0";

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

export function UpdateAllSheet({ staleTotal }: { staleTotal: number }) {
  const { t } = useTranslation();
  const dispatch = useFettlr((state) => state.dispatch);
  const progress = useFettlr((state) => state.progress);
  const busy = useFettlr((state) => state.busy);
  const [open, setOpen] = useState(false);
  const [phase, setPhase] = useState<Phase>("idle");
  const [failures, setFailures] = useState<UiUpdateFailure[]>([]);
  // Captured when the run starts: dispatch() clears `progress` in its finally, and staleTotal
  // itself drops to zero once the re-check lands, so neither can say what the run set out to do.
  const [attempted, setAttempted] = useState(0);

  // Not just `busy`: dispatch() resets it to false in a `finally` between the two calls below,
  // which would otherwise flash the done state mid-chain.
  const inFlight = phase === "updating" || phase === "refreshing";

  const runUpdateAll = async () => {
    const snapshot = await dispatch(commands.updateAll());
    if (!snapshot) {
      // dispatch() returns undefined on a total command failure (ErrorDialog already shows it):
      // never render success/failures from a snapshot that doesn't exist.
      setPhase("failed");
      return;
    }
    const itemFailures = snapshot.updateFailures;
    setFailures(itemFailures);
    setPhase("refreshing");
    // update_all's own refresh() carries the previous update report forward untouched, so
    // re-check here or the needs-updating list and sidebar badges stay stale after a real success.
    await dispatch(commands.checkUpdates());
    setPhase(itemFailures.length > 0 ? "partial" : "succeeded");
  };

  const start = () => {
    setOpen(true);
    setFailures([]);
    setAttempted(staleTotal);
    setPhase("updating");
    void runUpdateAll();
  };

  const showProgress = phase === "updating" && progress !== null;
  const showPreparing = phase === "updating" && progress === null;
  const settled = phase === "succeeded" || phase === "partial";
  const title = settled
    ? t("updateAll.titleDone")
    : phase === "failed"
      ? t("updateAll.titleFailed")
      : t("updateAll.title");
  const description = settled
    ? t("updateAll.summary", { done: attempted - failures.length, total: attempted })
    : t("updateAll.description");

  // A finished run leaves nothing stale, so the parent no longer has a reason to render this at
  // all. It stays mounted until the reader dismisses the sheet, or the summary it just produced
  // would be unmounted before it could be read.
  const dismiss = () => {
    setOpen(false);
    setPhase("idle");
  };

  useEffect(() => {
    if (phase !== "succeeded") return;
    const timer = window.setTimeout(dismiss, 3000);
    return () => window.clearTimeout(timer);
  }, [phase]);

  return (
    <>
      {staleTotal > 0 || phase !== "idle" ? (
        <Button type="button" onClick={start} disabled={staleTotal === 0 || busy || inFlight}>
          {inFlight ? (
            <RiLoader4Line className="size-4 animate-spin" aria-hidden="true" />
          ) : (
            <RiArrowUpCircleLine className="size-4" aria-hidden="true" />
          )}
          {t("updates.updateAll", { count: staleTotal })}
        </Button>
      ) : null}
      <Sheet
        open={open}
        onOpenChange={(next) => {
          if (inFlight) return;
          if (next) setOpen(true);
          else dismiss();
        }}
      >
        <SheetContent closeLabel={t("common.close")}>
          <SheetHeader>
            <SheetTitle>{title}</SheetTitle>
            <SheetDescription>{description}</SheetDescription>
          </SheetHeader>

          <SheetBody>
            {showProgress && progress ? (
              <div className={cn("flex flex-col gap-2", phaseIn)}>
                <div className="flex items-center gap-2 text-sm text-text">
                  <RiLoader4Line className="size-4 shrink-0 animate-spin text-accent-text" aria-hidden="true" />
                  <span className="truncate font-medium">{progress.name}</span>
                </div>
                <div className="h-1.5 w-full overflow-hidden rounded-full bg-bg-subtle">
                  <div
                    className="h-full w-full origin-left rounded-full bg-accent-text transition-transform"
                    style={{
                      transform: `scaleX(${progress.done / Math.max(progress.total, 1)})`,
                    }}
                  />
                </div>
                <p className="text-xs text-text-muted">
                  {t("updateAll.progress", {
                    done: Math.min(progress.done + 1, progress.total),
                    total: progress.total,
                  })}
                </p>
              </div>
            ) : null}

            {showPreparing ? (
              <div className={cn("flex items-center gap-2 text-sm text-text-muted", phaseIn)}>
                <RiLoader4Line className="size-4 shrink-0 animate-spin text-accent-text" aria-hidden="true" />
                {t("updateAll.preparing")}
              </div>
            ) : null}

            {phase === "refreshing" ? (
              <div className={cn("flex items-center gap-2 text-sm text-text-muted", phaseIn)}>
                <RiLoader4Line className="size-4 shrink-0 animate-spin text-accent-text" aria-hidden="true" />
                {t("common.refreshing")}
              </div>
            ) : null}

            {phase === "failed" ? (
              <div className={cn("flex items-center gap-2 text-sm font-medium text-danger-text", phaseIn)}>
                <RiAlertLine className="size-4" aria-hidden="true" />
                {t("updateAll.commandFailed")}
              </div>
            ) : null}

            {phase === "partial" ? (
              <div className={cn("flex flex-col gap-2", phaseIn)}>
                <div className="flex items-center gap-2 text-sm font-medium text-danger-text">
                  <RiAlertLine className="size-4" aria-hidden="true" />
                  {t("updateAll.failuresTitle")}
                </div>
                <ul className="flex flex-col gap-1.5 rounded-md border border-border bg-bg-subtle p-2.5 text-xs text-text-muted">
                  {failures.map((failure, index) => (
                    <li key={`${index}:${failure.kind}:${failure.name}`}>
                      {t(failureKey(failure.kind), { name: failure.name, error: failure.error })}
                    </li>
                  ))}
                </ul>
              </div>
            ) : null}

            {phase === "succeeded" ? (
              <div className={cn("flex items-center gap-2 text-sm text-success-text", phaseIn)}>
                <RiCheckboxCircleLine className="size-4" aria-hidden="true" />
                {t("updateAll.success")}
              </div>
            ) : null}
          </SheetBody>

          <SheetFooter>
            <Button
              type="button"
              variant="secondary"
              onClick={dismiss}
              disabled={inFlight}
            >
              {t("updateAll.done")}
            </Button>
          </SheetFooter>
        </SheetContent>
      </Sheet>
    </>
  );
}
