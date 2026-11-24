import { useEffect, useState } from 'react';

import {
  estimateExportMemory,
  type MemoryEstimate,
  type MemoryRequest,
} from '../ipc/export';

/** Trim handles fire on every drag step; ask Rust once the selection settles. */
const SETTLE_MS = 150;

/**
 * Rust's frame-memory estimate for the current selection, crop and settings: the same figures
 * `run_export` checks, so the export footer can warn and the status bar show the figure before a
 * run. Null with no source open (`request` null), until the first answer, and when Rust finds
 * the request impossible (Export reports that).
 */
export function useMemoryEstimate(
  request: MemoryRequest | null
): MemoryEstimate | null {
  // Each answer is kept with the source it was for, so a newly opened source never shows the
  // last one's figures while its own are on the way. A settings change keeps the old figure.
  const [answer, setAnswer] = useState<{
    source: string;
    estimate: MemoryEstimate | null;
  } | null>(null);
  // Effects compare dependencies by identity; the request is rebuilt every render.
  const key = request && JSON.stringify(request);
  const source = request && JSON.stringify(request.info);

  useEffect(() => {
    if (key === null) return;
    const { info } = JSON.parse(key) as MemoryRequest;
    const forSource = JSON.stringify(info);
    let current = true;
    const timer = setTimeout(() => {
      estimateExportMemory(JSON.parse(key) as MemoryRequest).then(
        (next) => current && setAnswer({ source: forSource, estimate: next }),
        () => current && setAnswer({ source: forSource, estimate: null })
      );
    }, SETTLE_MS);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [key]);

  return answer && answer.source === source ? answer.estimate : null;
}
