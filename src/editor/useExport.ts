import { useCallback, useEffect, useRef, useState } from 'react';

import {
  cancelExport,
  isExportError,
  onExportOutput,
  onExportProgress,
  runExport,
  type ExportOutput,
} from '../ipc/export';
import { fileUrl } from '../ipc/preview';
import { fileName } from './useSource';
import type { ExportJob } from './lib/export-run';
import { fileTypeInfo, formatBytes, type FileType } from './lib/output-settings';
import type { ExportRequest, SessionResult } from './lib/types';

/** What the editor says under Export when a run ends or cannot start. */
export type ExportNotice = {
  text: string;
  /** Shown in an expandable section: ffmpeg's error tail, or the reason Rust gave. */
  details?: string;
  failed: boolean;
};

let resultId = 0;

function toResult(output: ExportOutput): SessionResult | null {
  const id = `result-${++resultId}`;
  switch (output.status) {
    case 'done':
      return {
        id,
        fileType: output.fileType,
        path: output.path,
        name: fileName(output.path),
        url: fileUrl(output.path),
        size: output.size,
        width: output.width,
        height: output.height,
      };
    case 'failed':
      return { id, fileType: output.fileType, failed: true, reason: output.reason };
    case 'cancelled':
      // mgck lists nothing for a cancelled file type; its partial file is already deleted.
      return null;
  }
}

/** "GIF", "GIF and WebP", "GIF, WebP and MP4". */
function labels(fileTypes: readonly FileType[]): string {
  const names = fileTypes.map((type) => fileTypeInfo[type].label);
  return names.length > 1
    ? `${names.slice(0, -1).join(', ')} and ${names.at(-1)}`
    : (names[0] ?? '');
}

function summary(outputs: readonly ExportOutput[], folder: string): ExportNotice {
  const of = (status: ExportOutput['status']) =>
    outputs.filter((output) => output.status === status).map((o) => o.fileType);
  const [done, failed, cancelled] = [of('done'), of('failed'), of('cancelled')];
  const saved = done.length > 0 ? ` ${labels(done)} saved to ${folder}.` : '';
  if (cancelled.length > 0)
    return {
      text: `Cancelled; nothing unfinished was left behind.${saved}`,
      failed: false,
    };
  if (failed.length > 0)
    return {
      text: `${labels(failed)} failed; ffmpeg's error is in the Exports tab.${saved}`,
      failed: true,
    };
  return { text: `Saved ${labels(done)} to ${folder}.`, failed: false };
}

/** Words an ExportError, which stops a run before any file type starts. */
function startFailure(error: unknown): ExportNotice {
  if (!isExportError(error))
    return { text: 'Exporting failed.', details: String(error), failed: true };
  switch (error.kind) {
    case 'notFound':
      return { text: `The video is no longer at ${error.path}.`, failed: true };
    case 'destinationNotFound':
      return {
        text: `There is no folder at ${error.path} any more. Pick another with Save to.`,
        failed: true,
      };
    case 'invalidRequest':
      return {
        text: 'These settings cannot be exported.',
        details: error.reason,
        failed: true,
      };
    case 'ffmpegMissing':
      return {
        text: 'ffmpeg was not found on PATH, so nothing can be exported.',
        failed: true,
      };
    case 'notEnoughMemory':
      return {
        text: `This needs about ${formatBytes(error.heldBytes)} of memory for its frames and ${formatBytes(error.availableBytes)} is free. Trim the selection, or lower the width or frame rate.`,
        failed: true,
      };
    case 'busy':
      return { text: 'Another export is still running.', failed: true };
    case 'captionsFailed':
      return {
        text: 'The captions could not be prepared.',
        details: error.reason,
        failed: true,
      };
  }
}

/**
 * Runs exports through Rust's `run_export` and keeps what they made for the session. It lives
 * with the editor's page rather than one clip's editor, so results outlast the clip.
 */
export function useExport() {
  const [job, setJob] = useState<ExportJob | null>(null);
  const [results, setResults] = useState<SessionResult[]>([]);
  // The running export's settled file types; null between runs, when events are stale.
  const running = useRef<Set<FileType> | null>(null);

  const settle = useCallback((output: ExportOutput) => {
    const settled = running.current;
    if (!settled || settled.has(output.fileType)) return;
    settled.add(output.fileType);
    const result = toResult(output);
    // Newest first, as the Exports tab lists them.
    if (result) setResults((current) => [result, ...current]);
    setJob((current) => {
      if (!current) return current;
      const job = { ...current, settled: { ...current.settled, [output.fileType]: output } };
      // The next file type starts now; its first progress event may come a moment later.
      const next = job.fileTypes.find((type) => !job.settled[type]);
      return next && next !== job.fileType
        ? { ...job, fileType: next, done: 0, fileTypeStartedAt: Date.now() }
        : job;
    });
  }, []);

  // Subscribed for the page's lifetime, not per run: listen() registers asynchronously, so a
  // per-run subscription could miss a fast run's first events.
  useEffect(() => {
    const unlisten = Promise.all([
      onExportProgress(({ fileType, progress }) => {
        // Between runs events are stale; a settled file type's are late.
        if (!running.current || running.current.has(fileType)) return;
        setJob((current) => {
          if (!current) return current;
          if (fileType === current.fileType) return { ...current, done: progress };
          return { ...current, fileType, done: progress, fileTypeStartedAt: Date.now() };
        });
      }),
      onExportOutput(settle),
    ]);
    return () => {
      void unlisten.then((stops) => stops.forEach((stop) => stop()));
    };
  }, [settle]);

  const start = useCallback(
    async (
      request: ExportRequest,
      destination: string,
      estimates: Partial<Record<FileType, number>> = {}
    ): Promise<ExportNotice> => {
      const fileTypes = request.settings.fileTypes;
      running.current = new Set();
      const now = Date.now();
      setJob({
        fileTypes,
        // Export is disabled with no file type ticked, so there is a first one.
        fileType: fileTypes[0]!,
        done: 0,
        startedAt: now,
        fileTypeStartedAt: now,
        settled: {},
        estimates,
        cancelling: false,
      });
      try {
        const outputs = await runExport(request, destination);
        // Events normally settle each output first; this covers any that arrived late or never.
        outputs.forEach(settle);
        return summary(outputs, destination);
      } catch (error) {
        return startFailure(error);
      } finally {
        running.current = null;
        setJob(null);
      }
    },
    [settle]
  );

  const cancel = useCallback(() => {
    setJob((current) => current && { ...current, cancelling: true });
    // Rust settles the run's remaining outputs as cancelled and start() reports it.
    void cancelExport();
  }, []);

  return { job, results, start, cancel };
}
