import {
  Button,
  Input,
  Label,
  ScrollArea,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Switch,
  cn,
} from "@fettlr/design-system";
import {
  RiAlertLine,
  RiDownloadLine,
  RiLoader4Line,
  RiComputerLine,
  RiMoonLine,
  RiRefreshLine,
  RiRestartLine,
  RiSunLine,
  type RemixiconComponentType,
} from "@remixicon/react";
import { useEffect, useId, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { commands, type UiColorMode, type UiPalette, type UiPrefs } from "../../bindings";
import { setLanguage, type SupportedLanguage } from "../../i18n";
import { useFettlr } from "../../store";
import { ScreenShell, capsHeading, contentFrame, groupHeading } from "../screen";

const PROBE_DEBOUNCE_MS = 350;

const INTERVALS: { value: string; labelKey: string }[] = [
  { value: "manual", labelKey: "settings.intervalManual" },
  { value: "1", labelKey: "settings.interval1" },
  { value: "6", labelKey: "settings.interval6" },
  { value: "12", labelKey: "settings.interval12" },
  { value: "24", labelKey: "settings.interval24" },
];

const APPEARANCES: { value: UiColorMode; labelKey: string; icon: RemixiconComponentType }[] = [
  { value: "system", labelKey: "settings.themeSystem", icon: RiComputerLine },
  { value: "light", labelKey: "settings.themeLight", icon: RiSunLine },
  { value: "dark", labelKey: "settings.themeDark", icon: RiMoonLine },
];

const PALETTES: { value: UiPalette; labelKey: string }[] = [
  { value: "default", labelKey: "settings.paletteDefault" },
  { value: "highContrast", labelKey: "settings.paletteHighContrast" },
  { value: "dracula", labelKey: "settings.paletteDracula" },
];


/** Autonyms: a language's own name for itself, shown regardless of the app's current language. */
const LANGUAGES: { value: SupportedLanguage; label: string }[] = [
  { value: "en", label: "English (US)" },
  { value: "en-GB", label: "English (UK)" },
  { value: "fr", label: "Français" },
];

export function SettingsScreen() {
  return (
    <ScreenShell toolbar={<AppUpdateToolbarStatus />} actions={<AppUpdateToolbarAction />}>
      <ScrollArea className="min-h-0 flex-1" fillViewport>
        <div className="flex flex-1 flex-col gap-6 px-6 pb-6">
          <GeneralSection />
          <ClaudeCodeSection />
          <Colophon />
        </div>
      </ScrollArea>
    </ScreenShell>
  );
}

function AppUpdateToolbarStatus() {
  const { t } = useTranslation();
  const appUpdate = useFettlr((state) => state.appUpdate);
  const download = useFettlr((state) => state.download);

  let text = "";
  switch (appUpdate.kind) {
    case "current":
      text = t("settings.upToDate");
      break;
    case "available":
      text = t("settings.updateAvailable", { version: appUpdate.update.version });
      break;
    case "installing": {
      const total = download?.total ?? null;
      if (total && total > 0) {
        const percent = Math.min(100, Math.round(((download?.downloaded ?? 0) / total) * 100));
        text = t("settings.downloadingPercent", { percent });
      } else {
        text = t("settings.downloading");
      }
      break;
    }
    case "installed":
      text = t("settings.installed");
      break;
    case "failed":
      text = t("settings.checkFailed");
      break;
    case "idle":
      text = t("settings.notChecked");
      break;
    case "checking":
      text = t("settings.checking");
      break;
  }

  return (
    <p role="status" className="truncate text-sm text-text-muted">
      {text}
    </p>
  );
}

function AppUpdateToolbarAction() {
  const { t } = useTranslation();
  const appUpdate = useFettlr((state) => state.appUpdate);
  const checkAppUpdate = useFettlr((state) => state.checkAppUpdate);
  const installAppUpdate = useFettlr((state) => state.installAppUpdate);
  const dispatch = useFettlr((state) => state.dispatch);

  if (appUpdate.kind === "installing") return null;

  if (appUpdate.kind === "available") {
    return (
      <Button type="button" variant="secondary" onClick={() => void installAppUpdate()}>
        <RiDownloadLine className="size-4" aria-hidden="true" />
        {t("settings.install")}
      </Button>
    );
  }

  if (appUpdate.kind === "installed") {
    return (
      <Button type="button" variant="secondary" onClick={() => void dispatch(commands.relaunchApp())}>
        <RiRestartLine className="size-4" aria-hidden="true" />
        {t("settings.relaunch")}
      </Button>
    );
  }

  return (
    <Button
      type="button"
      variant="secondary"
      disabled={appUpdate.kind === "checking"}
      onClick={() => void checkAppUpdate(true)}
    >
      {appUpdate.kind === "checking" ? (
        <RiLoader4Line className="size-4 animate-spin" aria-hidden="true" />
      ) : (
        <RiRefreshLine className="size-4" aria-hidden="true" />
      )}
      {t("settings.checkNow")}
    </Button>
  );
}

function Section({
  titleKey,
  children,
}: {
  titleKey: string;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className={contentFrame}>
      <h2 id={headingId} className={cn(groupHeading, capsHeading, "flex items-center")}>
        {t(titleKey)}
      </h2>
      <div className="divide-y divide-border [&>*]:px-4">{children}</div>
    </section>
  );
}

function Row({
  label,
  description,
  htmlFor,
  control,
}: {
  label: string;
  description?: string;
  htmlFor?: string;
  control: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-6 py-3">
      <div className="flex min-w-0 flex-col gap-0.5">
        {htmlFor ? (
          <Label htmlFor={htmlFor} className="font-normal">
            {label}
          </Label>
        ) : (
          <span className="flex items-center gap-1.5 text-sm font-normal text-text select-none">
            {label}
          </span>
        )}
        {description ? <p className="text-xs text-text-muted">{description}</p> : null}
      </div>
      <div className="flex shrink-0 items-center gap-2">{control}</div>
    </div>
  );
}

function SegmentedField<T extends string>({
  name,
  legend,
  options,
  value,
  disabled,
  onChange,
}: {
  name: string;
  legend: string;
  options: { value: T; labelKey: string; icon: RemixiconComponentType }[];
  value: T;
  disabled: boolean;
  onChange: (value: T) => void;
}) {
  const { t } = useTranslation();
  return (
    <fieldset className="grid grid-cols-3 gap-0.5 rounded-md border border-border-strong bg-bg-subtle p-0.5">
      <legend className="sr-only">{legend}</legend>
      {options.map((option) => {
        const Icon = option.icon;
        const selected = value === option.value;
        return (
          <label
            key={option.value}
            className={cn(
              "relative flex items-center justify-center gap-1.5 rounded-sm",
              "px-2 py-1.5 text-sm transition-colors",
              "has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-solid",
              "has-[:focus-visible]:outline-offset-2 has-[:focus-visible]:outline-ring",
              selected
                ? "bg-selected text-selected-contrast"
                : "text-text-muted hover:text-text",
            )}
          >
            <input
              type="radio"
              name={name}
              value={option.value}
              checked={selected}
              disabled={disabled}
              onChange={() => onChange(option.value)}
              className="sr-only"
            />
            <Icon className="size-4" aria-hidden="true" />
            {t(option.labelKey)}
          </label>
        );
      })}
    </fieldset>
  );
}

function usePrefsWriter() {
  const prefs = useFettlr((state) => state.prefs);
  const savePrefs = useFettlr((state) => state.savePrefs);
  const write = (change: Partial<UiPrefs>) => {
    const current = useFettlr.getState().prefs;
    if (!current) return;
    void savePrefs({ ...current, ...change });
  };
  return { prefs, write };
}

function GeneralSection() {
  const { t, i18n } = useTranslation();
  const { prefs, write } = usePrefsWriter();
  const launchId = useId();
  const autoId = useId();

  return (
    <Section titleKey="settings.general">
      <div className="flex flex-col gap-2 py-3">
        <span className="text-sm text-text">{t("settings.appearance")}</span>
        <SegmentedField
          name="appearance"
          legend={t("settings.appearance")}
          options={APPEARANCES}
          value={prefs?.colorMode ?? "system"}
          disabled={!prefs}
          onChange={(value) => write({ colorMode: value })}
        />
      </div>

      <Row
        label={t("settings.theme")}
        control={
          <Select
            value={prefs?.palette ?? "default"}
            onValueChange={(next) => write({ palette: next as UiPalette })}
            disabled={!prefs}
          >
            <SelectTrigger className="w-52" aria-label={t("settings.theme")}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {PALETTES.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {t(option.labelKey)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        }
      />

      <Row
        label={t("settings.language")}
        control={
          <Select
            value={i18n.language}
            onValueChange={(next) => setLanguage(next as SupportedLanguage)}
          >
            <SelectTrigger className="w-52" aria-label={t("settings.language")}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {LANGUAGES.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        }
      />

      <Row
        label={t("settings.launchAtLogin")}
        description={t("settings.launchAtLoginHint")}
        htmlFor={launchId}
        control={
          <Switch
            id={launchId}
            checked={prefs?.launchAtLogin ?? false}
            disabled={!prefs}
            onCheckedChange={(checked) => write({ launchAtLogin: checked })}
          />
        }
      />

      <Row
        label={t("settings.autoCheck")}
        description={t("settings.autoCheckHint")}
        htmlFor={autoId}
        control={
          <Switch
            id={autoId}
            checked={prefs?.autoCheckForUpdates ?? true}
            disabled={!prefs}
            onCheckedChange={(checked) => write({ autoCheckForUpdates: checked })}
          />
        }
      />

      <Row
        label={t("settings.contentInterval")}
        description={t("settings.contentIntervalHint")}
        control={
          <Select
            value={prefs?.checkIntervalHours ? String(prefs.checkIntervalHours) : "manual"}
            disabled={!prefs}
            onValueChange={(next) =>
              write({ checkIntervalHours: next === "manual" ? null : Number(next) })
            }
          >
            <SelectTrigger className="w-52" aria-label={t("settings.contentInterval")}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {INTERVALS.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {t(option.labelKey)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        }
      />
    </Section>
  );
}

function ClaudeCodeSection() {
  const { t } = useTranslation();
  const { prefs, write } = usePrefsWriter();
  const pathId = useId();
  const [typed, setTyped] = useState<string | null>(null);
  const saved = prefs?.binaryOverride ?? "";
  const value = typed ?? saved;
  const dirty = typed !== null && typed.trim() !== saved.trim();
  const target = value.trim();
  const [probe, setProbe] = useState<{ path: string; found: string | null } | null>(null);

  /**
   * Debounced because a blank field falls all the way through to a login-shell probe, and the reader
   * types a path one character at a time.
   */
  useEffect(() => {
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void commands
        .probeBinary(target.length > 0 ? target : null)
        .then((result) => {
          if (cancelled) return;
          setProbe({ path: target, found: result.status === "ok" ? result.data : null });
        })
        .catch(() => {
          if (!cancelled) setProbe({ path: target, found: null });
        });
    }, PROBE_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [target]);

  const found = probe?.path === target ? probe.found : undefined;

  const apply = () => {
    if (!dirty) return;
    const trimmed = value.trim();
    write({ binaryOverride: trimmed.length > 0 ? trimmed : null });
    setTyped(null);
  };

  const placeholder = found === undefined ? "" : found === null ? t("settings.binaryPlaceholder") : found;

  return (
    <Section titleKey="settings.claudeCode">
      <div className="flex flex-col gap-2 py-3">
        <Label htmlFor={pathId} className="font-normal">
          {t("settings.binaryLabel")}
        </Label>
        <div className="flex items-center gap-2">
          <Input
            id={pathId}
            value={value}
            placeholder={placeholder}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            disabled={!prefs}
            className="font-mono"
            onChange={(event) => setTyped(event.target.value)}
            onKeyDown={(event) => {
              if (event.key !== "Enter") return;
              event.preventDefault();
              apply();
            }}
          />
          {dirty ? (
            <Button type="button" variant="secondary" onClick={apply}>
              {t("settings.binaryApply")}
            </Button>
          ) : null}
        </div>
        <BinaryIndicator found={found} />
      </div>
    </Section>
  );
}

function BinaryIndicator({ found }: { found: string | null | undefined }) {
  const { t } = useTranslation();

  if (found === undefined) {
    return (
      <p className="flex items-center gap-1.5 text-xs text-text-muted" role="status">
        <RiLoader4Line className="size-3.5 animate-spin" aria-hidden="true" />
        {t("settings.binaryChecking")}
      </p>
    );
  }
  if (found === null) {
    return (
      <p className="flex items-center gap-1.5 text-xs text-warning-text" role="status">
        <RiAlertLine className="size-3.5" aria-hidden="true" />
        {t("settings.binaryMissing")}
      </p>
    );
  }
  return null;
}

function Colophon() {
  const { t } = useTranslation();
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    void commands
      .appVersion()
      .then((result) => {
        if (result.status === "ok") setVersion(result.data);
      })
      .catch(() => undefined);
  }, []);

  return (
    <p className="mt-auto text-xs text-text-muted">
      {t("about.copyright", { year: String(new Date().getFullYear()) })}
      {version && (
        <>
          {" · "}
          {t("about.version", { name: t("app.name"), version })}
        </>
      )}
      {" · "}
      {t("about.license")}
    </p>
  );
}
