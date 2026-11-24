import { useState } from 'react';

import { isOutputError, openOutput, showOutputInFolder } from '../ipc/export';
import { SessionResult } from './lib/types';
import { fileTypeInfo, formatBytes } from './lib/advanced';

/**
 * Discord's upload limit for free accounts: 20 MB per file since August 2026
 * (Discord's File Attachments FAQ). Counted in decimal megabytes so the
 * warning errs early; Discord's UI has refused files near its stated caps.
 */
const discordFreeUploadLimit = { bytes: 20 * 1000 * 1000, label: '20 MB' };

/**
 * Gif Studio's *Made this session* list, as mgck shows it. Each output plays
 * in place through the asset protocol, and Open and Show in folder take the
 * place of mgck's Download link, since the file is already on disk.
 */
export default function SessionResults({
  results,
}: {
  results: readonly SessionResult[];
}) {
  return (
    <section aria-labelledby='session-results' className='flex flex-col gap-2'>
      <h3 id='session-results' className='text-sm font-semibold'>
        Made this session
      </h3>
      {results.length === 0 ? (
        <p className='text-sm text-foreground-dg/60'>
          Nothing yet. Your files show up here.
        </p>
      ) : (
        <ul className='flex flex-col gap-2'>
          {results.map((result) => (
            <li
              key={result.id}
              className='flex flex-col gap-2 rounded-md border border-foreground-dg/15 p-2'
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
    <div className='flex flex-col gap-1 text-left'>
      <p className='text-sm font-semibold text-red-500'>
        {fileTypeInfo[result.fileType].label} conversion failed
      </p>
      {result.reason && (
        <ErrorDetails summary="ffmpeg's error" details={result.reason} />
      )}
    </div>
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
    <details className='text-xs text-foreground-dg/80'>
      <summary className='cursor-pointer select-none'>{summary}</summary>
      <pre className='mt-1 max-h-40 overflow-auto whitespace-pre-wrap break-words rounded bg-background-dg p-2 font-mono'>
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
  const overLimit = result.size > discordFreeUploadLimit.bytes;

  const act = (action: (path: string) => Promise<void>) => () => {
    setProblem('');
    action(result.path).catch((error: unknown) =>
      setProblem(actionProblem(error, result.name))
    );
  };

  return (
    <>
      <div className='flex items-center gap-3'>
        <div className='grid h-20 w-28 shrink-0 place-items-center overflow-hidden rounded bg-black'>
          {result.fileType === 'mp4' ? (
            <video
              src={result.url}
              aria-label={`${label} preview`}
              className='max-h-full max-w-full'
              autoPlay
              loop
              muted
              playsInline
            />
          ) : (
            <img
              src={result.url}
              alt={`${label} preview`}
              className='max-h-full max-w-full'
            />
          )}
        </div>
        <div className='flex min-w-0 grow flex-col gap-1 text-left'>
          <p className='truncate text-sm' title={result.path}>
            {result.name}
          </p>
          <p className='text-xs text-foreground-dg/70'>
            <span className='font-semibold text-foreground-dg'>{label}</span> ·{' '}
            {result.width} × {result.height} ·{' '}
            <span className={overLimit ? 'text-amber-300' : undefined}>
              {formatBytes(result.size)}
            </span>
            {overLimit &&
              ` · over Discord's ${discordFreeUploadLimit.label} free upload limit`}
          </p>
        </div>
      </div>
      <div className='flex gap-2'>
        <button
          type='button'
          className='rounded-md bg-primary-dg px-3 py-1.5 text-sm font-semibold hover:bg-primary-dg/80'
          aria-label={`Open ${result.name}`}
          onClick={act(openOutput)}
        >
          Open
        </button>
        <button
          type='button'
          className='rounded-md border border-foreground-dg/25 px-3 py-1.5 text-sm hover:bg-foreground-dg/10'
          aria-label={`Show ${result.name} in folder`}
          onClick={act(showOutputInFolder)}
        >
          Show in folder
        </button>
      </div>
      {problem && (
        <p role='alert' className='text-xs text-red-500'>
          {problem}
        </p>
      )}
    </>
  );
}
