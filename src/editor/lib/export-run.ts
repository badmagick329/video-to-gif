// The running export as the Exports tab's running card and the footer's progress bar show it:
// where each ticked file type is, the overall progress, and the time left.
import type { ExportOutput } from '../../ipc/export';
import { fileTypeInfo, type FileType } from './output-settings';

/** The running export, from Export until `run_export` resolves. */
export type ExportJob = {
  /** The ticked file types in run order. */
  fileTypes: readonly FileType[];
  /** The one being made, or the last one once all have settled. */
  fileType: FileType;
  /** From 0 to 1 for that file type. */
  done: number;
  /** `Date.now()` at Export, and when the file type being made started. */
  startedAt: number;
  fileTypeStartedAt: number;
  /** Each file type that has settled, with how. */
  settled: Partial<Record<FileType, ExportOutput>>;
  /** The size estimate each file type had at Export, in bytes, where one was ready. */
  estimates: Partial<Record<FileType, number>>;
  cancelling: boolean;
};

/** Where one file type is in the run, for its row in the running card. */
export type RunRow =
  | { state: 'running'; done: number; secondsLeft: number | null; estimate?: number }
  | { state: 'queued' }
  | ExportOutput;

export function runRow(job: ExportJob, fileType: FileType, now: number): RunRow {
  const settled = job.settled[fileType];
  if (settled) return settled;
  if (fileType !== job.fileType) return { state: 'queued' };
  return {
    state: 'running',
    done: job.done,
    secondsLeft: secondsLeft(job.done, now - job.fileTypeStartedAt),
    estimate: job.estimates[fileType],
  };
}

/**
 * Seconds until the file type being made finishes, from its pace so far, or null while there is
 * too little to go on: under 2% done or under a second in, the guess swings wildly.
 */
export function secondsLeft(done: number, elapsedMs: number): number | null {
  if (done < 0.02 || elapsedMs < 1000) return null;
  return ((elapsedMs / 1000) * (1 - done)) / done;
}

/** `6.1 s left`, or `1:05 left` past a minute. */
export function formatSecondsLeft(seconds: number): string {
  return seconds < 60
    ? `${seconds.toFixed(1)} s left`
    : `${formatMinutes(Math.round(seconds))} left`;
}

/** Whole seconds as `m:ss`. */
export function formatMinutes(seconds: number): string {
  const whole = Math.max(0, Math.floor(seconds));
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}`;
}

/** The whole run's progress from 0 to 1: settled file types count in full. */
export function overallProgress(job: ExportJob): number {
  const settled = job.fileTypes.filter((type) => job.settled[type]).length;
  const running = job.settled[job.fileType] ? 0 : job.done;
  return Math.min(1, (settled + running) / job.fileTypes.length);
}

/** The footer bar's words: `Making WebP (2 of 3) · 42%`. */
export function progressText(job: ExportJob): string {
  const label = fileTypeInfo[job.fileType].label;
  const total = job.fileTypes.length;
  const index = Math.max(0, job.fileTypes.indexOf(job.fileType));
  const count = total > 1 ? ` (${index + 1} of ${total})` : '';
  // A GIF's palette pass reads every frame before the first is written.
  if (job.done === 0)
    return `Making ${label}${count}${job.fileType === 'gif' ? ', analysing colours' : ''}…`;
  return `Making ${label}${count} · ${Math.round(job.done * 100)}%`;
}
