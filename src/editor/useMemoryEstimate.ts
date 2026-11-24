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
 * `run_export` refuses with, so Convert can be disabled before the run would be refused. Null
 * until the first answer, and when Rust finds the request impossible (Convert reports that).
 */
export function useMemoryEstimate(request: MemoryRequest): MemoryEstimate | null {
  const [estimate, setEstimate] = useState<MemoryEstimate | null>(null);
  // Effects compare dependencies by identity; the request is rebuilt every render.
  const key = JSON.stringify(request);

  useEffect(() => {
    let current = true;
    const timer = setTimeout(() => {
      estimateExportMemory(JSON.parse(key) as MemoryRequest).then(
        (next) => current && setEstimate(next),
        () => current && setEstimate(null)
      );
    }, SETTLE_MS);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [key]);

  return estimate;
}
