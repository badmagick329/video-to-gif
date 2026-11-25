import clsx from 'clsx';
import { useCallback, useEffect, useState } from 'react';
import { LuX } from 'react-icons/lu';

import {
  forgetRecentSource,
  recentSources,
  type RecentSource,
} from '../ipc/preview';
import { fileName, type OpenFailure, type SourceState } from './useSource';

/**
 * Recent, loaded as the start view appears, so a source opened since the last time shows and a
 * file deleted since is gone. Recent is a convenience: when it cannot be read the list is empty.
 */
function useRecent() {
  const [sources, setSources] = useState<readonly RecentSource[]>([]);
  useEffect(() => {
    let current = true;
    recentSources().then(
      (loaded) => {
        if (current) setSources(loaded);
      },
      () => {}
    );
    return () => {
      current = false;
    };
  }, []);
  const forget = useCallback((path: string) => {
    setSources((list) => list.filter((source) => source.path !== path));
    // Stays off the list this session either way; a failed write only brings it back next launch.
    forgetRecentSource(path).catch(() => {});
  }, []);
  return { sources, forget };
}

/**
 * The viewer with nothing open, as board `Start.dc.html` draws it: the drop zone, which lights
 * up while a file is dragged over the window, Open video, and Recent. While a source opens it
 * says so; when one fails to open it says why, with ffmpeg's reason folded.
 */
export default function StartView({
  state,
  dragging,
  onChoose,
  onOpen,
}: {
  state: Exclude<SourceState, { stage: 'open' }>;
  dragging: boolean;
  /** Opens the file dialog. */
  onChoose: () => void;
  /** Opens a path from Recent, the way a dialog pick is opened. */
  onOpen: (path: string) => void;
}) {
  const { sources, forget } = useRecent();
  return (
    <div
      className={clsx(
        'flex min-h-0 flex-1 flex-col self-stretch overflow-y-auto rounded-xl border-[1.5px] border-dashed transition-colors',
        dragging ? 'border-accent bg-accent/5' : 'border-[#34373e]'
      )}
    >
      <div className='m-auto flex flex-col items-center gap-3.5 px-6 py-8'>
        {state.stage === 'probing' ? (
          <p aria-busy='true' className='text-lg font-medium'>
            Opening {fileName(state.path)}…
          </p>
        ) : (
          <>
            {state.stage === 'failed' && <Failure failure={state.failure} />}
            <svg
              aria-hidden
              width='44'
              height='44'
              viewBox='0 0 24 24'
              fill='none'
              stroke={dragging ? 'rgb(var(--accent))' : '#6f747e'}
              strokeWidth='1.5'
            >
              <rect x='2.5' y='5' width='19' height='14' rx='2' />
              <path d='M10 9.5l4.5 2.5-4.5 2.5z' />
            </svg>
            <p className='text-lg font-medium'>
              Drop a video or GIF anywhere in this window
            </p>
            <p className='text-dim'>
              Anything your ffmpeg can read, at any size. The file is opened in
              place, never copied.
            </p>
            <button
              type='button'
              className='flex h-[38px] items-center gap-2.5 rounded-[7px] bg-accent px-[18px] text-[13.5px] font-semibold text-accent-fg hover:bg-accent-hover'
              onClick={onChoose}
            >
              Open video…
              <span
                aria-hidden
                className='rounded border border-[#5a2c08]/45 px-1 font-mono text-2xs text-[#5a2c08]'
              >
                Ctrl O
              </span>
            </button>
            {sources.length > 0 && (
              <Recent sources={sources} onOpen={onOpen} onForget={forget} />
            )}
          </>
        )}
      </div>
    </div>
  );
}

/** A failed open: what went wrong in plain words, the tool's message folded under it. */
function Failure({ failure }: { failure: OpenFailure }) {
  return (
    <div
      role='alert'
      className='mb-2 flex w-[560px] max-w-full flex-col gap-1.5 rounded-lg border border-danger/40 bg-danger/10 px-3.5 py-2.5 text-left'
    >
      <p className='font-medium text-danger'>{failure.text}</p>
      {failure.detail && (
        <details className='text-sm text-muted'>
          <summary className='cursor-pointer select-none hover:text-fg'>
            {failure.detail.label}
          </summary>
          <pre className='mt-1.5 max-h-40 overflow-auto whitespace-pre-wrap break-words rounded bg-well px-2 py-1.5 font-mono text-2xs text-fg-soft'>
            {failure.detail.text}
          </pre>
        </details>
      )}
    </div>
  );
}

function Recent({
  sources,
  onOpen,
  onForget,
}: {
  sources: readonly RecentSource[];
  onOpen: (path: string) => void;
  onForget: (path: string) => void;
}) {
  // One clock for the whole list, read as it renders.
  const now = new Date();
  return (
    <nav
      aria-label='Recent videos'
      className='mt-[22px] flex w-[560px] max-w-full flex-col text-left'
    >
      <h2 className='px-2.5 pb-1.5 text-xs font-semibold uppercase tracking-[.07em] text-dim'>
        Recent
      </h2>
      <ul className='flex flex-col'>
        {sources.map((source) => {
          const name = fileName(source.path);
          return (
            <li
              key={source.path}
              className='group flex items-center rounded-[7px] hover:bg-[#1b1c20]'
            >
              <button
                type='button'
                title={source.path}
                className='grid min-w-0 flex-1 grid-cols-[minmax(0,1fr)_auto] items-center gap-x-4 gap-y-0.5 py-2 pl-2.5 text-left hover:text-white'
                onClick={() => onOpen(source.path)}
              >
                <span className='truncate'>{name}</span>
                <span className='text-right font-mono text-sm text-dim'>
                  {formatDuration(source.duration)}
                </span>
                <span className='truncate text-[11.5px] text-dim'>
                  {folderOf(source.path)}
                </span>
                <span className='whitespace-nowrap text-right text-[11.5px] text-faint'>
                  {formatOpened(source.openedAt, now)}
                </span>
              </button>
              <button
                type='button'
                aria-label={`Remove ${name} from Recent`}
                title='Remove from Recent'
                className='mx-1.5 grid h-6 w-6 shrink-0 place-items-center rounded text-dim opacity-0 hover:bg-hover hover:text-fg focus-visible:opacity-100 group-hover:opacity-100'
                onClick={() => onForget(source.path)}
              >
                <LuX aria-hidden className='h-3.5 w-3.5' />
              </button>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}

function folderOf(path: string): string {
  const cut = Math.max(path.lastIndexOf('\\'), path.lastIndexOf('/'));
  return cut > 0 ? path.slice(0, cut) : path;
}

/** A source's length to the second, as `41:17` or `1:02:03`. */
export function formatDuration(seconds: number): string {
  const total = Math.round(Math.max(0, seconds));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = String(total % 60).padStart(2, '0');
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${s}` : `${m}:${s}`;
}

const months = [
  'Jan',
  'Feb',
  'Mar',
  'Apr',
  'May',
  'Jun',
  'Jul',
  'Aug',
  'Sep',
  'Oct',
  'Nov',
  'Dec',
];

/** When a source was opened, relative to `now` in local time: `Today 14:32`, `2 Nov`. */
export function formatOpened(openedAt: number, now: Date): string {
  const opened = new Date(openedAt);
  const day = (date: Date) =>
    new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  const days = Math.round((day(now) - day(opened)) / 86_400_000);
  const time = `${String(opened.getHours()).padStart(2, '0')}:${String(opened.getMinutes()).padStart(2, '0')}`;
  if (days === 0) return `Today ${time}`;
  if (days === 1) return `Yesterday ${time}`;
  const date = `${opened.getDate()} ${months[opened.getMonth()]}`;
  return opened.getFullYear() === now.getFullYear()
    ? date
    : `${date} ${opened.getFullYear()}`;
}
