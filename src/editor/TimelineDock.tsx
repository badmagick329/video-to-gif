import { PointerEvent, useEffect, useRef, useState } from 'react';
import { LuZoomIn } from 'react-icons/lu';

import TimelineTrack from './TimelineTrack';
import { useElementSize } from './Viewer';
import {
  useRangeThumbnails,
  useTimelineThumbnails,
} from './useTimelineThumbnails';
import { formatClipTime, parseClipTime } from './lib/trim';
import {
  isZoomed,
  panBy,
  shortestSpan,
  showTime,
  spanAtLevel,
  wholeView,
  zoomLevel,
  zoomTo,
  type TimelineView,
} from './lib/timeline';

type Selection = { start: number; end: number };

const button =
  'inline-flex h-7 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-md px-1.5 text-md text-fg-soft hover:bg-hover hover:text-fg disabled:cursor-not-allowed disabled:opacity-50';

/** The zoom slider's steps from the whole clip to the deepest zoom. */
const zoomSteps = 1000;

/** Ctrl+wheel zooms by e^(this × deltaY): one 100-pixel notch is about 22%. */
const wheelZoomRate = 0.002;

/** Pixels a wheel line counts for, when the wheel reports lines rather than pixels. */
const wheelLine = 16;

/** The track's width before it is measured, and in jsdom, which has no layout. */
const unmeasuredWidth = 1000;

/**
 * The timeline dock under the viewer: a header row with the selection's In and Out times, its
 * length, Set in, Set out, Whole video and the zoom slider, over the ruler and track
 * (`TimelineTrack`) and, when zoomed, a scrollbar.
 *
 * The dock owns the view, the span of the source the ruler and track show. Zooming keeps the
 * playhead where it sits; Ctrl+wheel zooms, the wheel scrolls a zoomed view. When the playhead
 * moves out of view (playback running on, a jump to the out point), the view pages to it, except
 * for the dock's own seeks: releasing the out handle returns the playhead to the in point without
 * taking the view with it.
 */
export default function TimelineDock({
  path,
  src,
  total,
  frameSeconds,
  selection,
  trimmed,
  currentTime,
  disabled,
  playError,
  onSelectionChange,
  onSeek,
  onScrubChange,
  onCommitIn,
  onCommitOut,
  onSetIn,
  onSetOut,
}: {
  /** The source's path, which thumbnails are asked for by. */
  path: string;
  /** The preview source's URL; a proxy replacing the source re-requests thumbnails. */
  src: string;
  total: number;
  frameSeconds: number;
  selection: Selection;
  /** Whether the selection is less than the whole clip, so Whole video has work to do. */
  trimmed: boolean;
  currentTime: number;
  disabled: boolean;
  playError: string;
  /** A new selection, or undefined for the whole clip. */
  onSelectionChange: (selection: Selection | undefined) => void;
  onSeek: (seconds: number) => void;
  onScrubChange: (scrubbing: boolean) => void;
  /** A typed In or Out time. */
  onCommitIn: (seconds: number) => void;
  onCommitOut: (seconds: number) => void;
  /** Set in and Set out: the in or out point at the playhead. */
  onSetIn: () => void;
  onSetOut: () => void;
}) {
  const [view, setView] = useState<TimelineView>(() => wholeView(total));
  // The playhead time the view last accounted for; see `follow` below.
  const [shownTime, setShownTime] = useState(currentTime);
  const area = useRef<HTMLDivElement>(null);
  const { width } = useElementSize(area, {
    width: unmeasuredWidth,
    height: 0,
  });
  const thumbnails = useTimelineThumbnails(path, src);
  const rangeStrip = useRangeThumbnails(path, src, view, total);
  const zoomed = isZoomed(view, total);

  // Follow the playhead when something else moves it out of view. Adjusted while rendering,
  // React's pattern for state derived from a changing prop. The tolerance absorbs the player
  // reporting a seek's time back a hair off.
  if (Math.abs(currentTime - shownTime) > 0.001) {
    setShownTime(currentTime);
    const next = showTime(view, currentTime, total, frameSeconds);
    if (next !== view) setView(next);
  }
  const seekHere = (seconds: number) => {
    setShownTime(seconds);
    onSeek(seconds);
  };

  // React's wheel listeners are passive, and Ctrl+wheel must not zoom the page, so the listener
  // is attached by hand. It reads the latest props through a ref.
  const latest = useRef({ view, currentTime, total, frameSeconds, width, disabled });
  useEffect(() => {
    latest.current = { view, currentTime, total, frameSeconds, width, disabled };
  });
  useEffect(() => {
    const element = area.current;
    if (!element) return;
    const onWheel = (event: WheelEvent) => {
      const { view, currentTime, total, frameSeconds, width, disabled } =
        latest.current;
      if (disabled) return;
      if (event.ctrlKey || event.metaKey) {
        event.preventDefault();
        const factor = Math.exp(event.deltaY * wheelZoomRate);
        setView((view) =>
          zoomTo(view, view.span * factor, currentTime, total, frameSeconds)
        );
        return;
      }
      // Unzoomed, the wheel is left to whatever else wants it.
      if (!isZoomed(view, total)) return;
      event.preventDefault();
      const sideways = Math.abs(event.deltaX) > Math.abs(event.deltaY);
      const pixels =
        (sideways ? event.deltaX : event.deltaY) *
        (event.deltaMode === 1 ? wheelLine : 1);
      setView((view) =>
        panBy(view, (pixels / width) * view.span, total, frameSeconds)
      );
    };
    element.addEventListener('wheel', onWheel, { passive: false });
    return () => element.removeEventListener('wheel', onWheel);
  }, []);

  const length = selection.end - selection.start;

  return (
    <section
      aria-label='Timeline'
      className='flex h-[180px] shrink-0 flex-col overflow-hidden border-t border-line bg-dock'
    >
      <fieldset disabled={disabled} className='flex min-w-0 flex-col'>
        <legend className='sr-only'>Selection</legend>
        <div className='flex h-[38px] items-center gap-1.5 overflow-hidden px-3'>
          <span className='shrink-0 text-sm font-semibold'>Selection</span>
          <TimeField label='In' seconds={selection.start} onCommit={onCommitIn} />
          <TimeField label='Out' seconds={selection.end} onCommit={onCommitOut} />
          <p className='shrink-0 whitespace-nowrap text-sm text-dim'>
            length{' '}
            <span className='font-mono text-base font-semibold tabular-nums text-fg'>
              {length.toFixed(2)} s
            </span>
          </p>
          <span className='flex-1' />
          <button type='button' className={button} onClick={onSetIn}>
            Set in <span aria-hidden className='kbd'>I</span>
          </button>
          <button type='button' className={button} onClick={onSetOut}>
            Set out <span aria-hidden className='kbd'>O</span>
          </button>
          <button
            type='button'
            className={button}
            disabled={!trimmed}
            onClick={() => onSelectionChange(undefined)}
          >
            Whole video
          </button>
          <LuZoomIn aria-hidden className='h-3.5 w-3.5 shrink-0 text-dim' />
          <input
            type='range'
            aria-label='Timeline zoom'
            title='Zoom (Ctrl + wheel)'
            min={0}
            max={zoomSteps}
            step={1}
            value={Math.round(
              zoomLevel(view.span, total, frameSeconds) * zoomSteps
            )}
            disabled={disabled || shortestSpan(total, frameSeconds) >= total}
            className='w-[90px] min-w-[56px] shrink accent-dim'
            onChange={(event) =>
              setView(
                zoomTo(
                  view,
                  spanAtLevel(
                    Number(event.target.value) / zoomSteps,
                    total,
                    frameSeconds
                  ),
                  currentTime,
                  total,
                  frameSeconds
                )
              )
            }
          />
        </div>
        <div ref={area} className='mx-4'>
          <TimelineTrack
            total={total}
            view={view}
            width={width}
            selection={selection}
            currentTime={currentTime}
            frameSeconds={frameSeconds}
            thumbnails={thumbnails}
            rangeStrip={rangeStrip}
            disabled={disabled}
            onSelectionChange={onSelectionChange}
            onSeek={seekHere}
            onScrubChange={onScrubChange}
          />
          <Scrollbar
            view={view}
            total={total}
            hidden={!zoomed}
            onScroll={(start) =>
              setView((view) =>
                panBy(view, start - view.start, total, frameSeconds)
              )
            }
          />
        </div>
        {playError && (
          <p role='alert' className='px-4 pt-1 text-sm text-danger'>
            {playError}
          </p>
        )}
      </fieldset>
    </section>
  );
}

/**
 * The zoomed timeline's horizontal scrollbar: the thumb is the view's share of the clip. Dragging
 * it scrolls; pressing beside it centres the view there first. The wheel does the same for
 * keyboard-and-wheel users, so it is left out of the accessibility tree.
 */
function Scrollbar({
  view,
  total,
  hidden,
  onScroll,
}: {
  view: TimelineView;
  total: number;
  hidden: boolean;
  /** The view's new start, in seconds. */
  onScroll: (start: number) => void;
}) {
  const drag = useRef<{ pointer: number; x: number; start: number; width: number }>();
  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    const rect = event.currentTarget.getBoundingClientRect();
    if (rect.width <= 0) return;
    let start = view.start;
    if (!(event.target as HTMLElement).dataset.thumb) {
      const at = ((event.clientX - rect.left) / rect.width) * total;
      start = at - view.span / 2;
      onScroll(start);
    }
    drag.current = {
      pointer: event.pointerId,
      x: event.clientX,
      start,
      width: rect.width,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
    event.preventDefault();
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const active = drag.current;
    if (!active || active.pointer !== event.pointerId) return;
    onScroll(active.start + ((event.clientX - active.x) / active.width) * total);
  };
  const endDrag = () => {
    drag.current = undefined;
  };
  return (
    <div
      aria-hidden
      data-testid='timeline-scrollbar'
      className={`relative mt-2 h-2 touch-none rounded bg-well ${hidden ? 'invisible' : ''}`}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onLostPointerCapture={endDrag}
    >
      <div
        data-thumb='true'
        className='absolute inset-y-0 min-w-[16px] rounded bg-line-strong hover:bg-dim'
        style={{
          left: `${(view.start / total) * 100}%`,
          width: `${(view.span / total) * 100}%`,
        }}
      />
    </div>
  );
}

/** Holds a draft while typing so partial input is never clamped mid-edit. */
function TimeField({
  label,
  seconds,
  onCommit,
}: {
  label: string;
  seconds: number;
  onCommit: (seconds: number) => void;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const commit = () => {
    if (draft === null) return;
    const parsed = parseClipTime(draft);
    setDraft(null);
    if (parsed !== null) onCommit(parsed);
  };
  return (
    <label className='inline-flex h-[26px] shrink-0 items-center gap-1.5 rounded-[5px] border border-line-control bg-canvas pl-2 text-xs uppercase text-dim focus-within:border-accent'>
      {label}
      <input
        type='text'
        inputMode='decimal'
        spellCheck={false}
        autoComplete='off'
        className='h-full w-[10ch] bg-transparent pr-2 font-mono text-base normal-case text-fg focus:outline-none'
        value={draft ?? formatClipTime(seconds)}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === 'Enter') commit();
          if (event.key === 'Escape') setDraft(null);
        }}
      />
    </label>
  );
}
