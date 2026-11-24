import type { MemoryEstimate } from '../ipc/export';
import { ErrorDetails } from './SessionResults';
import type { Destination } from './useDestination';
import type { ExportNotice } from './useExport';
import type { ExportJob } from './lib/export-run';
import type { FileTypeEstimate } from './useSizeEstimate';
import {
  OutputSettings,
  FileType,
  fileTypeInfo,
  fileTypeOrder,
  formatBytes,
} from './lib/output-settings';
import { formatClipTime } from './lib/trim';

/** The Save to row's state and actions, from `useDestination`. */
export type SaveTo = {
  destination: Destination | null;
  error: string;
  choose: () => Promise<void>;
  saveBesideSource: () => Promise<void>;
};

/**
 * The inspector's pinned footer: each ticked file type's size estimate, the frame-memory warning,
 * Save to, and Export, which turns into the run's progress and Cancel while an export runs.
 */
export default function ExportFooter({
  settings,
  seconds,
  frames,
  estimates,
  playing,
  memory,
  saveTo,
  converting,
  job,
  notice,
  onExport,
  onCancel,
}: {
  settings: OutputSettings;
  /** Output seconds and frames, for the estimate's header. */
  seconds: number;
  frames: number;
  /** Each ticked file type's size estimate. */
  estimates: Partial<Record<FileType, FileTypeEstimate>>;
  /** Estimates wait while the video plays. */
  playing: boolean;
  memory: MemoryEstimate | null;
  saveTo: SaveTo;
  /** An export is starting or running, from this clip or an earlier one. */
  converting: boolean;
  job: ExportJob | null;
  /** What the last export from this clip ended with. */
  notice: ExportNotice | null;
  onExport: () => void;
  onCancel: () => void;
}) {
  const ticked = fileTypeOrder.filter((type) =>
    settings.fileTypes.includes(type)
  );
  // Held frames past free memory make Windows page. The warning says so and Export becomes
  // Export anyway, which tells Rust the user accepted it; the user decides.
  const paging = memory !== null && !memory.fits;
  const canExport =
    !converting && ticked.length > 0 && saveTo.destination !== null;
  const label =
    ticked.length === 0
      ? 'Export'
      : paging
        ? 'Export anyway'
        : `Export ${ticked.map((type) => fileTypeInfo[type].label).join(' + ')}`;

  return (
    <div className='flex shrink-0 flex-col gap-2.5 border-t border-[#2a2d33] bg-raised px-3.5 pb-3.5 pt-3'>
      <div role='group' aria-label='Estimated size' className='flex flex-col gap-1'>
        <p className='flex justify-between gap-2 text-xs font-semibold uppercase tracking-[.07em] text-dim'>
          <span>Estimated size</span>
          <span className='font-normal normal-case tracking-normal'>
            {frames.toLocaleString('en')} frames · {formatLength(seconds)}
          </span>
        </p>
        {ticked.map((type) => {
          const estimate = estimates[type];
          return (
            estimate && <EstimateRow key={type} fileType={type} estimate={estimate} />
          );
        })}
        {ticked.length === 0 ? (
          <p className='text-md text-dim'>Pick at least one format</p>
        ) : (
          <EstimateNote estimates={estimates} playing={playing} />
        )}
      </div>
      {memory && <MemoryWarning estimate={memory} settings={settings} />}
      <SaveToRow saveTo={saveTo} disabled={converting} />
      {job ? (
        <div className='flex flex-col gap-2'>
          <ExportProgress job={job} />
          <button
            type='button'
            className='h-8 rounded-md border border-line-strong bg-control text-md hover:bg-control-hover disabled:opacity-50'
            disabled={job.cancelling}
            onClick={onCancel}
          >
            {job.cancelling ? 'Cancelling…' : 'Cancel'}
          </button>
        </div>
      ) : (
        <button
          type='button'
          aria-keyshortcuts='Control+Enter'
          className='flex h-[38px] items-center justify-center gap-2.5 rounded-[7px] bg-accent text-[13.5px] font-semibold text-accent-fg hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:bg-accent'
          disabled={!canExport}
          onClick={onExport}
        >
          {label}
          <span
            aria-hidden
            className='rounded border border-[#5a2c08]/45 px-1 font-mono text-2xs text-[#5a2c08]'
          >
            Ctrl Enter
          </span>
        </button>
      )}
      {notice && (
        <div
          role={notice.failed ? 'alert' : 'status'}
          className='flex flex-col gap-1 text-sm text-muted'
        >
          <p className={notice.failed ? 'text-danger' : undefined}>
            {notice.text}
          </p>
          {notice.details && (
            <ErrorDetails summary='Details' details={notice.details} />
          )}
        </div>
      )}
    </div>
  );
}

/** Seconds as the header shows them: `14.2 s`, or `6:50.0` past a minute. */
function formatLength(seconds: number): string {
  return seconds < 60
    ? `${seconds.toFixed(1)} s`
    : formatClipTime(Math.round(seconds * 10) / 10).replace(/0$/, '');
}

/**
 * One file type's expected size. The figure is the whole message: no limit or judgement. While
 * a newer estimate runs, the old figure stays, greyed, rather than blanking.
 */
function EstimateRow({
  fileType,
  estimate,
}: {
  fileType: FileType;
  estimate: FileTypeEstimate;
}) {
  const label = fileTypeInfo[fileType].label;
  const row = 'flex items-baseline justify-between text-md';
  const figure = 'font-mono text-base font-medium';
  switch (estimate.state) {
    case 'estimating':
      return (
        <p className={row}>
          <span>{label}</span>{' '}
          <span
            role='img'
            aria-label='Estimating'
            className='inline-block h-3 w-16 self-center rounded-[3px] bg-gradient-to-r from-hover via-[#30333a] to-hover motion-safe:animate-pulse'
          />
        </p>
      );
    case 'failed':
      return (
        <p className={row} title={estimate.reason}>
          <span>{label}</span>{' '}
          <span className='text-dim'>size unknown</span>
        </p>
      );
    case 'ready':
    case 'stale':
      return (
        <p
          className={row}
          aria-label={`${label} about ${formatBytes(estimate.bytes)}${estimate.state === 'stale' ? ', updating' : ''}`}
        >
          <span>{label}</span>{' '}
          <b
            className={
              estimate.state === 'stale' ? `${figure} text-faint` : figure
            }
          >
            ≈ {formatBytes(estimate.bytes)}
          </b>
        </p>
      );
  }
}

/** What the size figures are and whether they are up to date. */
function EstimateNote({
  estimates,
  playing,
}: {
  estimates: Partial<Record<FileType, FileTypeEstimate>>;
  playing: boolean;
}) {
  const states = Object.values(estimates).map((estimate) => estimate.state);
  if (states.length === 0) return null;
  const pending = states.some(
    (state) => state === 'estimating' || state === 'stale'
  );
  let text = 'Sizes come from samples of this selection, so expect some drift.';
  if (pending && playing) text = 'Sizes update when the video stops.';
  else if (states.includes('stale')) text = 'Updating sizes…';
  else if (pending) text = 'Encoding short samples of the selection…';
  return <p className='text-[11.5px] text-dim'>{text}</p>;
}

/** The export's progress bar: the file type being made, which of how many, and how far. */
function ExportProgress({ job }: { job: ExportJob }) {
  const index = Math.max(0, job.fileTypes.indexOf(job.fileType));
  const progress = (index + job.done) / job.fileTypes.length;
  return (
    <div
      role='progressbar'
      aria-label='Export progress'
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(progress * 100)}
      className='relative h-[38px] overflow-hidden rounded-[7px] bg-control'
    >
      <div
        className='absolute inset-y-0 left-0 bg-accent/70 transition-[width]'
        style={{ width: `${progress * 100}%` }}
      />
      <p className='relative grid h-full place-items-center text-md font-semibold'>
        {progressText(job, index)}
      </p>
    </div>
  );
}

function progressText(job: ExportJob, index: number): string {
  const label = fileTypeInfo[job.fileType].label;
  const total = job.fileTypes.length;
  const count = total > 1 ? ` (${index + 1}/${total})` : '';
  // A GIF's palette pass reads every frame before the first is written.
  if (job.done === 0)
    return `Making ${label}${count}${job.fileType === 'gif' ? ', analysing colours' : ''}…`;
  return `Making ${label}${count} ${Math.round(job.done * 100)}%`;
}

/** Where Export saves, with Save to's folder picker. */
function SaveToRow({ saveTo, disabled }: { saveTo: SaveTo; disabled: boolean }) {
  const { destination, error, choose, saveBesideSource } = saveTo;
  const button =
    'inline-flex h-6 shrink-0 items-center rounded-md border border-line-strong bg-control px-2 text-sm hover:bg-control-hover disabled:opacity-50';
  return (
    <div role='group' aria-label='Save to' className='flex flex-col gap-1'>
      <div className='flex items-center gap-2 text-sm'>
        <span className='text-dim'>Save to</span>{' '}
        <span
          className='min-w-0 flex-1 truncate'
          title={
            destination?.from === 'source'
              ? `${destination.folder} (the video's folder)`
              : destination?.folder
          }
        >
          {destination?.folder ?? '…'}
        </span>{' '}
        <button
          type='button'
          className={button}
          disabled={disabled}
          onClick={() => void choose()}
        >
          Change…
        </button>
      </div>
      {destination?.from === 'chosen' && (
        <button
          type='button'
          className='self-start text-sm text-accent-soft hover:text-[#ffb985] disabled:opacity-50'
          disabled={disabled}
          onClick={() => void saveBesideSource()}
        >
          Use the video&apos;s folder
        </button>
      )}
      {error && (
        <p role='alert' className='text-sm text-danger'>
          {error}
        </p>
      )}
    </div>
  );
}

/**
 * The frame-memory guard's estimate, worded for this machine's free memory: the budget is what
 * Windows reports free. It warns and never blocks.
 */
function MemoryWarning({
  estimate,
  settings,
}: {
  estimate: MemoryEstimate;
  settings: OutputSettings;
}) {
  const { heldBytes, availableBytes, limitSeconds, fits } = estimate;
  if (heldBytes === 0 || availableBytes === null) return null;
  const holder = memoryHolder(settings.direction);
  const held = formatBytes(heldBytes);
  const free = formatBytes(availableBytes);
  const box =
    'rounded-md border border-warn/30 bg-warn/[.08] px-2.5 py-2 text-sm leading-[1.45] text-warn';
  if (!fits)
    return (
      <p role='alert' className={box}>
        {holder} holds about {held} of frames in memory and {free} is free, so
        Windows will page to disk and the PC may crawl.{' '}
        {limitSeconds !== null && limitSeconds > 0
          ? `Lower the width or frame rate, or trim to ${formatClipTime(Math.floor(limitSeconds * 100) / 100)} or less, to stay in memory.`
          : 'Lower the width or frame rate to stay in memory.'}
      </p>
    );
  // Fits, but a run this big can push the rest of the PC into its page file.
  if (heldBytes > availableBytes / 2)
    return (
      <p className={box}>
        {holder} will keep about {held} of frames in memory, more than half of
        the {free} free now.
      </p>
    );
  return null;
}

/** What holds the frames: the direction, or else the GIF's palette pass. */
function memoryHolder(direction: OutputSettings['direction']): string {
  if (direction === 'reverse') return 'Reverse';
  if (direction === 'boomerang') return 'Boomerang';
  return 'Making a GIF';
}
