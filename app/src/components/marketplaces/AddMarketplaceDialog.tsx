import {
  Button,
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
} from "@fettlr/design-system";
import { RiLoader4Line } from "@remixicon/react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { commands } from "../../bindings";
import { useFettlr } from "../../store";

export function AddMarketplaceDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const dispatch = useFettlr((state) => state.dispatch);
  const [source, setSource] = useState("");
  const [submitting, setSubmitting] = useState(false);

  const trimmed = source.trim();

  const handleSubmit = async () => {
    if (!trimmed) return;
    setSubmitting(true);
    try {
      const result = await dispatch(commands.addMarketplace(trimmed));
      if (result) {
        setSource("");
        onOpenChange(false);
      }
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!submitting) onOpenChange(next);
      }}
    >
      <DialogContent closeLabel={t("common.close")}>
        <DialogHeader>
          <DialogTitle>{t("marketplaces.addDialog.title")}</DialogTitle>
          <DialogDescription>{t("marketplaces.addDialog.description")}</DialogDescription>
        </DialogHeader>

        <DialogBody>
          <form
            className="flex flex-col gap-2"
            onSubmit={(event) => {
              event.preventDefault();
              void handleSubmit();
            }}
          >
            <Label htmlFor="marketplace-source">{t("marketplaces.addDialog.sourceLabel")}</Label>
            <Input
              id="marketplace-source"
              value={source}
              onChange={(event) => setSource(event.target.value)}
              placeholder={t("marketplaces.addDialog.sourcePlaceholder")}
              disabled={submitting}
              autoFocus
            />
          </form>
        </DialogBody>

        <DialogFooter>
          <Button
            type="button"
            variant="secondary"
            onClick={() => onOpenChange(false)}
            disabled={submitting}
          >
            {t("common.cancel")}
          </Button>
          <Button type="button" disabled={submitting || !trimmed} onClick={() => void handleSubmit()}>
            {submitting ? <RiLoader4Line className="size-4 animate-spin" aria-hidden="true" /> : null}
            {t("marketplaces.addDialog.submit")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
