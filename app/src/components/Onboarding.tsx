import { Button, Input, Label, ScrollArea } from "@fettlr/design-system";
import { RiTerminalBoxLine } from "@remixicon/react";
import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";

import { commands } from "../bindings";
import { useFettlr } from "../store";

export function Onboarding() {
  const { t } = useTranslation();
  const dispatch = useFettlr((state) => state.dispatch);
  const select = useFettlr((state) => state.select);
  const busy = useFettlr((state) => state.busy);
  const warnings = useFettlr((state) => state.snapshot?.warnings ?? []);

  const prefs = useFettlr((state) => state.prefs);
  const savePrefs = useFettlr((state) => state.savePrefs);
  const [submitting, setSubmitting] = useState(false);
  const [typedPath, setTypedPath] = useState<string | null>(null);
  const binaryPath = typedPath ?? prefs?.binaryOverride ?? "";

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!prefs) return;
    const trimmed = binaryPath.trim();
    const next = trimmed || null;
    setSubmitting(true);
    try {
      const ok = await savePrefs({ ...prefs, binaryOverride: next });
      if (ok && next === (prefs.binaryOverride ?? null)) await dispatch(commands.refresh());
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <ScrollArea className="h-full min-h-0">
      <div className="flex min-h-full flex-col items-center justify-center gap-6 px-6 py-16">
        <div className="flex size-12 items-center justify-center rounded-full bg-accent-soft">
          <RiTerminalBoxLine className="size-6 text-accent-text" aria-hidden="true" />
        </div>

        <div className="flex max-w-prose flex-col gap-2 text-center">
          <h1 className="text-2xl font-semibold tracking-tight text-text">{t("onboarding.title")}</h1>
          <p className="text-base text-text-muted">{t("onboarding.description")}</p>
        </div>

        <form onSubmit={(event) => void handleSubmit(event)} className="flex w-full max-w-sm flex-col gap-3">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="binary-path">{t("onboarding.binaryLabel")}</Label>
            <Input
              id="binary-path"
              name="binary-path"
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              placeholder={t("onboarding.binaryPlaceholder")}
              value={binaryPath}
              onChange={(event) => setTypedPath(event.target.value)}
              disabled={!prefs || submitting}
            />
            <p className="text-xs text-text-muted">{t("onboarding.binaryHint")}</p>
          </div>
          <Button type="submit" disabled={!prefs || busy || submitting}>
            {t("onboarding.retry")}
          </Button>
        </form>

        {warnings.length > 0 ? (
          <ul className="flex w-full max-w-sm flex-col gap-1 rounded-md border border-border bg-bg-subtle p-3 text-xs text-text-muted">
            {warnings.map((warning, index) => (
              <li key={`${index}:${warning}`}>{warning}</li>
            ))}
          </ul>
        ) : null}

        <div className="flex flex-col items-center gap-1 text-center">
          <p className="text-xs text-text-muted">{t("onboarding.browsePrompt")}</p>
          <Button type="button" variant="link" size="sm" onClick={() => select("skills")}>
            {t("onboarding.browseAction")}
          </Button>
        </div>
      </div>
    </ScrollArea>
  );
}
