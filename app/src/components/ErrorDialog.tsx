import {
  Button,
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@fettlr/design-system";
import { RiCheckLine, RiFileCopyLine, RiErrorWarningLine } from "@remixicon/react";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { useFettlr } from "../store";

export function ErrorDialog() {
  const { t } = useTranslation();
  const error = useFettlr((state) => state.error);
  const clearError = useFettlr((state) => state.clearError);
  const [showDetail, setShowDetail] = useState(false);
  const [copied, setCopied] = useState(false);
  const detailRef = useRef<HTMLPreElement>(null);

  const dismiss = () => {
    setShowDetail(false);
    setCopied(false);
    clearError();
  };

  return (
    <Dialog
      open={error !== null}
      onOpenChange={(open) => {
        if (!open) dismiss();
      }}
    >
      <DialogContent closeLabel={t("common.close")}>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <RiErrorWarningLine className="size-5 text-danger-text" aria-hidden="true" />
            {t("error.title")}
          </DialogTitle>
          <DialogDescription>{error?.message}</DialogDescription>
        </DialogHeader>

        {error?.detail ? (
          <DialogBody className="gap-2">
            <div className="flex items-center justify-between gap-2">
              <Button
                type="button"
                variant="link"
                size="sm"
                className="h-auto self-start p-0"
                onClick={() => {
                  setShowDetail((value) => !value);
                  setCopied(false);
                }}
              >
                {showDetail ? t("error.hideDetail") : t("error.showDetail")}
              </Button>
              {showDetail ? (
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() => setCopied(selectAndCopy(detailRef.current))}
                >
                  {copied ? (
                    <RiCheckLine className="size-4" aria-hidden="true" />
                  ) : (
                    <RiFileCopyLine className="size-4" aria-hidden="true" />
                  )}
                  {t(copied ? "common.copied" : "common.copy")}
                </Button>
              ) : null}
            </div>
            {showDetail ? (
              <pre
                ref={detailRef}
                className="max-h-48 overflow-auto rounded-md border border-border bg-bg-subtle p-2.5 font-mono text-xs break-words whitespace-pre-wrap text-text-muted"
              >
                {error.detail}
              </pre>
            ) : null}
          </DialogBody>
        ) : null}

        <DialogFooter>
          <Button type="button" variant="secondary" onClick={dismiss}>
            {t("common.close")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// The window runs on a custom protocol, which WebKit does not treat as a secure context, so
// navigator.clipboard is undefined in a shipped build. Selecting the text and asking the document
// to copy works without one, and if even that is refused the text is left selected, so the reader
// can still copy the stderr this dialog exists to hand them.
function selectAndCopy(node: HTMLElement | null): boolean {
  const selection = window.getSelection();
  if (!node || !selection) return false;
  const range = document.createRange();
  range.selectNodeContents(node);
  selection.removeAllRanges();
  selection.addRange(range);
  try {
    const copied = document.execCommand("copy");
    // Left selected only when the copy was refused, so the reader can still press Cmd-C. Leaving
    // it selected after it worked just looks like the click did something else.
    if (copied) selection.removeAllRanges();
    return copied;
  } catch {
    return false;
  }
}
