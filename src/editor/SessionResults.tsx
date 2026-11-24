import { useState } from 'react';

import { isOutputError, openOutput, showOutputInFolder } from '../ipc/export';
import { sourceFolder } from './useDestination';
import { SessionResult } from './lib/types';
import { fileTypeInfo, formatBytes } from './lib/output-settings';

/**
 * The Exports tab's *This session* list: every output this session made, newest first, and every
 * file type that failed. Each output shows a thumbnail drawn from its file, its name, then format,
 * pixel size and bytes, with Open and Show in folder. It belongs to the window, not the clip, so
 * opening another video keeps it.
 */
export default function SessionResults({
  results,
}: {
  /** Newest first. */
  results: readonly SessionResult[];
}) {
  const newest = results.find((result) => !('failed' in result));
  return (
    <section aria-labelledby='session-results' className='flex flex-col gap-1.5'>
      <h3
        id='session-results'
        className='flex justify-between gap-3 text-xs font-semibold uppercase tracking-[.07em] text-dim'
      >
        This session
        {newest && 'path' in newest && (
          // Where the latest output went; usually where the rest did too.
          <span
            className='min-w-0 truncate font-normal normal-case tracking-normal'
            title={sourceFolder(newest.path)}
          >
            {sourceFolder(newest.path)}
          </span>
        )}
      </h3>
      {results.length === 0 ? (
        <p className='text-md text-dim'>Nothing yet. Your files show up here.</p>
      ) : (
        <ul className='flex flex-col gap-0.5'>
          {results.map((result) => (
            <li
              key={result.id}
              className='grid grid-cols-[56px_minmax(0,1fr)] gap-2.5 rounded-lg p-2 hover:bg-raised'
            >
              {'failed' in result ? (
                <FailedRow result={result} />
              ) : (
                <OutputRow result={result} />
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function FailedRow({
  result,
}: {
  result: Extract<SessionResult, { failed: true }>;
}) {
  return (
    <>
      <div
        aria-hidden
        className='grid h-[62px] w-14 place-items-center rounded bg-danger/10 text-lg font-semibold text-danger'
      >
        !
      </div>
      <div className='flex min-w-0 flex-col gap-1'>
        <p className='text-md font-medium text-danger'>
          {fileTypeInfo[result.fileType].label} failed
        </p>
        {result.reason && (
          <ErrorDetails summary="ffmpeg's error" details={result.reason} />
        )}
      </div>
    </>
  );
}

/** The long part of a failure, folded away: ffmpeg's error tail or Rust's reason. */
export function ErrorDetails({
  summary,
  details,
}: {
  summary: string;
  details: string;
}) {
  return (
    <details className='text-xs text-fg/80'>
      <summary className='cursor-pointer select-none'>{summary}</summary>
      <pre className='mt-1 max-h-40 overflow-auto whitespace-pre-wrap break-words rounded bg-canvas p-2 font-mono'>
        {details}
      </pre>
    </details>
  );
}

function actionProblem(error: unknown, name: string): string {
  if (!isOutputError(error)) return String(error);
  switch (error.kind) {
    case 'missing':
      return `${name} has been moved or deleted.`;
    case 'notAnOutput':
      return `${name} was not made in this session.`;
    case 'shellFailed':
      return `Windows could not do that: ${error.reason}`;
  }
}

function OutputRow({
  result,
}: {
  result: Exclude<SessionResult, { failed: true }>;
}) {
  const [problem, setProblem] = useState('');
  const label = fileTypeInfo[result.fileType].label;

  const act = (action: (path: string) => Promise<void>) => () => {
    setProblem('');
    action(result.path).catch((error: unknown) =>
      setProblem(actionProblem(error, result.name))
    );
  };
  const button =
    'inline-flex h-6 items-center rounded-[5px] border border-line-strong bg-control px-2 text-[11.5px] hover:bg-control-hover';
  const thumbnail = 'block h-[62px] w-14 rounded bg-black object-cover';

  return (
    <>
      {result.fileType === 'mp4' ? (
        // The first frame stands still as the thumbnail.
        <video
          src={result.url}
          aria-label={`${label} preview`}
          className={thumbnail}
          preload='metadata'
          muted
          playsInline
        />
      ) : (
        <img src={result.url} alt={`${label} preview`} className={thumbnail} />
      )}
      <div className='min-w-0'>
        <p className='truncate text-md' title={result.path}>
          {result.name}
        </p>
        <p className='mt-0.5 text-[11.5px] text-muted'>
          {label} · {result.width} × {result.height} ·{' '}
          <span className='font-mono text-fg-label'>{formatBytes(result.size)}</span>
        </p>
        <div className='mt-1.5 flex flex-wrap gap-1'>
          <button
            type='button'
            className={button}
            aria-label={`Open ${result.name}`}
            onClick={act(openOutput)}
          >
            Open
          </button>
          <button
            type='button'
            className={button}
            aria-label={`Show ${result.name} in folder`}
            onClick={act(showOutputInFolder)}
          >
            Show in folder
          </button>
        </div>
        {problem && (
          <p role='alert' className='mt-1 text-xs text-danger'>
            {problem}
          </p>
        )}
      </div>
    </>
  );
}
