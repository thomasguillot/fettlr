import { useState } from "react";

import type { SectionId } from "../sections";
import { useFettlr } from "../store";

export function useRevealReset(section: SectionId, visibleIds: string[], reset: () => void) {
  const request = useFettlr((state) => state.revealRequest);
  const [seenSeq, setSeenSeq] = useState(request?.seq ?? 0);
  if (request && request.section === section && request.seq !== seenSeq) {
    setSeenSeq(request.seq);
    if (!visibleIds.includes(request.id)) reset();
  }
}
