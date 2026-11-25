import clsx from 'clsx';
import {
  ReactNode,
  RefObject,
  useCallback,
  useEffect,
  useState,
} from 'react';

import type { Size } from './lib/output-preview';

/** What the viewer's stage shows: the source with its crop, the output preview, or both. */
export type ViewerMode = 'source' | 'output' | 'split';

const viewerModes: ReadonlyArray<{ value: ViewerMode; label: string }> = [
  { value: 'source', label: 'Source' },
  { value: 'output', label: 'Output' },
  { value: 'split', label: 'Side by side' },
];

const modeKey = 'video-to-gif.viewer-mode';

function savedMode(): ViewerMode {
  try {
    const saved = localStorage.getItem(modeKey);
    if (saved === 'source' || saved === 'output' || saved === 'split')
      return saved;
  } catch {
    // Storage can be unavailable; the default is fine.
  }
  return 'split';
}

/**
 * The viewer mode, remembered across launches in the webview's local storage (WebView2 keeps it
 * in the app's profile). Side by side until the user picks another.
 */
export function useViewerMode(): [ViewerMode, (mode: ViewerMode) => void] {
  const [mode, setMode] = useState<ViewerMode>(savedMode);
  const choose = useCallback((next: ViewerMode) => {
    setMode(next);
    try {
      localStorage.setItem(modeKey, next);
    } catch {
      // Not remembered this time; the session still uses it.
    }
  }, []);
  return [mode, choose];
}

/** The viewer's header: the mode switch, and the crop controls on the right. */
export function ViewerHeader({
  mode,
  onModeChange,
  children,
}: {
  mode: ViewerMode;
  onModeChange: (mode: ViewerMode) => void;
  children?: ReactNode;
}) {
  return (
    <div className='flex h-9 shrink-0 items-center justify-between gap-3 border-b border-line-subtle px-3'>
      <Segmented
        label='Viewer mode'
        options={viewerModes}
        value={mode}
        onChange={onModeChange}
      />
      {children}
    </div>
  );
}

/** The board's segmented control: one pressed button among a few. */
export function Segmented<T extends string>({
  label,
  options,
  value,
  onChange,
  className,
}: {
  label: string;
  options: ReadonlyArray<{ value: T; label: string }>;
  value: T;
  onChange: (value: T) => void;
  className?: string;
}) {
  return (
    <div
      role='group'
      aria-label={label}
      className={clsx(
        'flex gap-0.5 rounded-[7px] border border-line-control bg-well p-0.5',
        className
      )}
    >
      {options.map((option) => (
        <button
          key={option.value}
          type='button'
          aria-pressed={value === option.value}
          className='h-6 flex-1 whitespace-nowrap rounded-[5px] px-2.5 text-sm text-[#a4a8b1] hover:text-fg aria-pressed:bg-line-strong aria-pressed:font-medium aria-pressed:text-white'
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

/** A pane's caption above its picture, as the board draws it. */
export function PaneLabel({ children }: { children: ReactNode }) {
  return (
    <p className='self-start whitespace-nowrap text-xs uppercase tracking-[.06em] text-dim'>
      {children}
    </p>
  );
}

/** Height a pane's label takes, with its gap, out of the stage. */
export const paneLabelHeight = 22;

/** The largest size of `aspect` (width over height) inside `area`. */
export function fitAspect(area: Size, aspect: number): Size {
  const width = Math.max(1, Math.min(area.width, area.height * aspect));
  return { width, height: Math.max(1, width / aspect) };
}

/**
 * An element's content-box size, kept current as the window resizes. Without ResizeObserver
 * (jsdom) it reports `fallback`.
 */
export function useElementSize(
  element: RefObject<HTMLElement>,
  fallback: Size
): Size {
  const [size, setSize] = useState<Size>(fallback);
  useEffect(() => {
    const target = element.current;
    if (!target || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(([entry]) => {
      if (!entry) return;
      const { width, height } = entry.contentRect;
      setSize((current) =>
        current.width === width && current.height === height
          ? current
          : { width, height }
      );
    });
    observer.observe(target);
    return () => observer.disconnect();
  }, [element]);
  return size;
}

/**
 * The left column with no player in it: the viewer's stage showing `children`, over an empty
 * timeline area. Used while nothing is open and while a proxy builds.
 */
export function PlaceholderColumn({
  header,
  children,
}: {
  header?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className='flex min-h-0 min-w-0 flex-col'>
      <section
        aria-label='Viewer'
        className='flex min-h-0 flex-1 flex-col bg-viewer'
      >
        {header}
        <div className='flex min-h-0 flex-1 flex-col items-center justify-center gap-3 overflow-y-auto bg-stage p-4 text-center'>
          {children}
        </div>
      </section>
      <section
        aria-label='Timeline'
        className='flex h-[180px] shrink-0 flex-col gap-2.5 border-t border-line bg-dock px-4 py-3'
      >
        <p aria-hidden className='text-sm text-faint'>
          Timeline
        </p>
        {/* The board's ghost track: an empty strip with faint slice lines. */}
        <div className='h-[72px] rounded-md bg-[repeating-linear-gradient(90deg,#17181b_0_112px,#1a1b1f_112px_113px)]' />
      </section>
    </div>
  );
}

/** Shown on the stage while ffmpeg makes the proxy. */
export function PreviewCopyProgress({
  name,
  progress,
}: {
  name: string;
  progress: number;
}) {
  const percent = Math.round(progress * 100);
  return (
    <div className='flex w-full max-w-md flex-col gap-2'>
      <p>
        Making a preview copy of {name}: {percent}%
      </p>
      <div
        role='progressbar'
        aria-label='Preview copy progress'
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent}
        className='relative h-1.5 overflow-hidden rounded-full bg-control'
      >
        <div
          className='absolute inset-y-0 left-0 bg-accent transition-[width]'
          style={{ width: `${percent}%` }}
        />
      </div>
      <p className='text-sm text-muted'>
        The player can&apos;t show this file as it is, so ffmpeg makes a
        smaller copy to trim and crop against. Your files are still made from
        the original.
      </p>
    </div>
  );
}
