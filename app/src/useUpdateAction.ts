import { useCallback, useState } from "react";

import { commands, type UiSnapshot } from "./bindings";
import { useFettlr } from "./store";

export function useUpdateAction() {
  const dispatch = useFettlr((s) => s.dispatch);
  const [running, setRunning] = useState<string | null>(null);

  // Spans both dispatch() calls, not just `busy` (which resets to false in the gap between them),
  // so buttons don't flicker back on mid-chain. The re-check mirrors update_all: the mutation's own
  // refresh() carries the previous report forward, so a successful update must check again.
  const run = useCallback(
    async (key: string, mutate: () => Promise<UiSnapshot | undefined>) => {
      setRunning(key);
      try {
        if (await mutate()) await dispatch(commands.checkUpdates());
      } finally {
        setRunning(null);
      }
    },
    [dispatch],
  );

  return { run, running, isRunning: (key: string) => running === key };
}
