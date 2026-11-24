import { useEffect, useState } from 'react';

import {
  formatMinutes,
  formatSecondsLeft,
  runRow,
  type ExportJob,
  type RunRow,
} from './lib/export-run';
import { fileTypeInfo, formatBytes, type FileType } from './lib/output-settings';

/**
 * The Exports tab's card for the running export, like a render queue: a row per ticked file type
 * in run order. A finished one shows a ✓ and its actual size; the one being made its percent, time
 * left, size estimate and a bar; the rest Queued.
 */
export default function RunningCard({ job }: { job: ExportJob }) {
  // Time left and "started … ago" move with the clock, not only with progress events.
  const now = useNow(1000);
  return (
    <section aria-labelledby='running-export' className='flex flex-col gap-2'>
      <h3
        id='running-export'
        className='flex justify-between text-xs font-semibold uppercase tracking-[.07em] text-dim'
      >
        Running
        <span className='font-normal normal-case tracking-normal'>
          started {formatMinutes((now - job.startedAt) / 1000)} ago
        </span>
      </h3>
      <ul className='flex flex-col gap-2 rounded-lg border border-line-control bg-raised p-2.5'>
        {job.fileTypes.map((fileType) => (
          <Row key={fileType} fileType={fileType} row={runRow(job, fileType, now)} />
        ))}
      </ul>
    </section>
  );
}

function Row({ fileType, row }: { fileType: FileType; row: RunRow }) {
  const label = fileTypeInfo[fileType].label;
  const grid = 'grid grid-cols-[44px_1fr_auto] items-center gap-2 text-md';
  const mono = 'font-mono text-[11.5px] text-fg-label';
  if ('status' in row) {
    switch (row.status) {
      case 'done':
        return (
          <li className={grid} aria-label={`${label} done, ${formatBytes(row.size)}`}>
            <b className='font-semibold'>{label}</b>
            <span className={mono}>{formatBytes(row.size)}</span>
            <span aria-hidden className='text-ok'>
              ✓
            </span>
          </li>
        );
      case 'failed':
        return (
          <li className={grid}>
            <b className='font-semibold'>{label}</b>
            <span className='text-danger'>Failed</span>
            <span />
          </li>
        );
      case 'cancelled':
        return (
          <li className={`${grid} text-dim`}>
            <b className='font-semibold text-fg-soft'>{label}</b>
            <span>Cancelled</span>
            <span />
          </li>
        );
    }
  }
  if (row.state === 'queued')
    return (
      <li className={`${grid} text-dim`}>
        <b className='font-semibold text-fg-soft'>{label}</b>
        <span>Queued</span>
        <span />
      </li>
    );
  const percent = Math.round(row.done * 100);
  return (
    <li className={grid}>
      <b className='font-semibold'>{label}</b>
      <span className={mono}>
        {percent}%{row.secondsLeft !== null && ` · ${formatSecondsLeft(row.secondsLeft)}`}
      </span>
      <span className={`${mono} text-dim`}>
        {row.estimate !== undefined && `est ${formatBytes(row.estimate)}`}
      </span>
      <div
        role='progressbar'
        aria-label={`${label} progress`}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent}
        className='col-span-2 col-start-2 h-1 overflow-hidden rounded-sm bg-[#2a2d33]'
      >
        <div className='h-full bg-accent' style={{ width: `${percent}%` }} />
      </div>
    </li>
  );
}

/** `Date.now()`, refreshed every `intervalMs`. */
function useNow(intervalMs: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(timer);
  }, [intervalMs]);
  return now;
}
