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
import { fileTypeInfo, formatBytes, type FileType } from './lib/advanced';
import type { ExportRequest, SessionResult } from './lib/types';

/** mgck's `Job`: the running export, for its per-file-type progress bar. */
export type ExportJob = {
  /** The ticked file types in run order. */
  fileTypes: readonly FileType[];
  /** The one being made. */
  fileType: FileType;
  /** From 0 to 1 for that file type. */
  done: number;
  cancelling: boolean;
};

/** What the editor says under Convert when a run ends or cannot start. */
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
      text: `${labels(failed)} failed; ffmpeg's error is under Made this session.${saved}`,
      failed: true,
    };
  return { text: `Saved ${labels(done)} to ${folder}.`, failed: false };
}

/** Words an ExportError, which stops a run before any file type starts. */
function startFailure(error: unknown): ExportNotice {
  if (!isExportError(error))
    return { text: 'Converting failed.', details: String(error), failed: true };
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
        text: 'These settings cannot be converted.',
        details: error.reason,
        failed: true,
      };
    case 'ffmpegMissing':
      return {
        text: 'ffmpeg was not found on PATH, so nothing can be converted.',
        failed: true,
      };
    case 'notEnoughMemory':
      return {
        text: `This needs about ${formatBytes(error.heldBytes)} of memory for its frames and ${formatBytes(error.availableBytes)} is free. Trim the selection, or lower the width or frame rate.`,
        failed: true,
      };
    case 'busy':
      return { text: 'Another conversion is still running.', failed: true };
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
    if (result) setResults((current) => [...current, result]);
  }, []);

  // Subscribed for the page's lifetime, not per run: listen() registers asynchronously, so a
  // per-run subscription could miss a fast run's first events.
  useEffect(() => {
    const unlisten = Promise.all([
      onExportProgress(({ fileType, progress }) => {
        if (!running.current) return;
        setJob((current) => current && { ...current, fileType, done: progress });
      }),
      onExportOutput(settle),
    ]);
    return () => {
      void unlisten.then((stops) => stops.forEach((stop) => stop()));
    };
  }, [settle]);

  const start = useCallback(
    async (request: ExportRequest, destination: string): Promise<ExportNotice> => {
      const fileTypes = request.settings.fileTypes;
      running.current = new Set();
      // Convert is disabled with no file type ticked, so there is a first one.
      setJob({ fileTypes, fileType: fileTypes[0]!, done: 0, cancelling: false });
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
