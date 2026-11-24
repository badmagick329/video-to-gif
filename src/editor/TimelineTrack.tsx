import { KeyboardEvent, PointerEvent, useRef } from 'react';
import {
  formatClipTime,
  moveSelection,
  setSelectionEnd,
  setSelectionStart,
  shortestSelection,
  snapToFrame,
} from './lib/trim';
import { rulerTicks, type TimelineView } from './lib/timeline';
import type { RangeStrip } from './useTimelineThumbnails';

type Selection = { start: number; end: number };
type Part = 'start' | 'end' | 'span' | 'playhead' | 'ruler' | 'seek';

/** Pointer travel, in pixels, below which a press on the span is a click. */
const moveThreshold = 3;

/**
 * Positions outside the view are clamped to just past the track's edges, where `overflow:
 * hidden` clips them; at the deepest zoom of a long clip the raw share runs to millions of
 * percent.
 */
const offTrack = 10;

const clamp = (value: number, min: number, max: number) =>
  Math.min(max, Math.max(min, value));

/**
 * The timeline dock's ruler and track for one `view` of the source: ruler ticks, the thumbnail
 * strip, the selection with in and out handles and the outside dimmed, and a playhead across
 * both. Pressing or dragging on the ruler scrubs; on the track it seeks, drags a handle or moves
 * the span. Pointer edits land on frame boundaries (`snapToFrame`). The handles and playhead are
 * sliders, so keyboard users get the same edits, a frame per arrow.
 */
export default function TimelineTrack({
  total,
  view,
  width,
  selection,
  currentTime,
  frameSeconds,
  thumbnails,
  rangeStrip,
  disabled,
  onSelectionChange,
  onSeek,
  onScrubChange,
}: {
  total: number;
  view: TimelineView;
  /** The track's measured width in pixels, which sets the ruler's tick spacing. */
  width: number;
  selection: Selection;
  currentTime: number;
  frameSeconds: number;
  /** The whole source's thumbnails, in time order. */
  thumbnails: string[];
  /** A denser strip of the view, drawn over the whole-source one. */
  rangeStrip: RangeStrip | null;
  disabled: boolean;
  onSelectionChange: (selection: Selection) => void;
  onSeek: (seconds: number) => void;
  onScrubChange: (scrubbing: boolean) => void;
}) {
  const surface = useRef<HTMLDivElement>(null);
  const drag = useRef<{
    pointer: number;
    part: Part;
    left: number;
    width: number;
    view: TimelineView;
    originX: number;
    origin: number;
    selection: Selection;
    moved: boolean;
  }>();
  const { start, end } = selection;
  const viewEnd = view.start + view.span;
  /** A time's place on the track, as a percentage of its width. */
  const share = (seconds: number) => ((seconds - view.start) / view.span) * 100;
  const place = (seconds: number) =>
    `${clamp(share(seconds), -offTrack, 100 + offTrack)}%`;
  const shortest = shortestSelection(total, frameSeconds);
  const ticks = rulerTicks(view, width, frameSeconds);

  const applyStart = (seconds: number, from = selection) => {
    const next = setSelectionStart(from, seconds, total, frameSeconds);
    onSelectionChange(next);
    onSeek(next.start);
  };
  const applyEnd = (seconds: number, from = selection) => {
    const next = setSelectionEnd(from, seconds, total, frameSeconds);
    onSelectionChange(next);
    onSeek(next.end);
  };

  /** The frame boundary under the pointer, in the view the drag started in. */
  const timeAt = (clientX: number, from: NonNullable<typeof drag.current>) =>
    clamp(
      snapToFrame(
        from.view.start +
          clamp((clientX - from.left) / from.width, 0, 1) * from.view.span,
        frameSeconds
      ),
      0,
      total
    );

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (disabled || event.button !== 0) return;
    const rect = surface.current!.getBoundingClientRect();
    if (rect.width <= 0) return;
    const target = (event.target as HTMLElement).closest<HTMLElement>(
      '[data-part]'
    );
    const part = (target?.dataset.part as Part | undefined) ?? 'seek';
    const active = {
      pointer: event.pointerId,
      part,
      left: rect.left,
      width: rect.width,
      view,
      originX: event.clientX,
      origin: 0,
      selection,
      moved: false,
    };
    active.origin = timeAt(event.clientX, active);
    drag.current = active;
    event.currentTarget.setPointerCapture(event.pointerId);
    event.preventDefault();
    target?.focus();
    if (part === 'playhead' || part === 'ruler') onScrubChange(true);
    if (part === 'seek' || part === 'ruler') onSeek(active.origin);
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const active = drag.current;
    if (disabled || !active || active.pointer !== event.pointerId) return;
    const seconds = timeAt(event.clientX, active);
    if (Math.abs(event.clientX - active.originX) > moveThreshold)
      active.moved = true;
    if (active.part === 'start') applyStart(seconds, active.selection);
    else if (active.part === 'end') applyEnd(seconds, active.selection);
    else if (active.part === 'span' && active.moved) {
      const next = moveSelection(
        active.selection,
        snapToFrame(seconds - active.origin, frameSeconds),
        total
      );
      onSelectionChange(next);
      onSeek(next.start);
    } else if (active.part !== 'span') onSeek(seconds);
  };

  const endDrag = () => {
    const active = drag.current;
    if (!active) return;
    drag.current = undefined;
    // A press on the span that never moved is a click, so it seeks like the rest of the track.
    if (active.part === 'span' && !active.moved) onSeek(active.origin);
    // After placing the end, return to the start so the next play shows the whole selection.
    if (active.part === 'end') onSeek(active.selection.start);
    if (active.part === 'playhead' || active.part === 'ruler')
      onScrubChange(false);
  };

  const onKeyDown = (
    event: KeyboardEvent<HTMLDivElement>,
    part: 'start' | 'end' | 'playhead'
  ) => {
    if (disabled) return;
    const step = event.shiftKey ? 1 : frameSeconds;
    const value = part === 'start' ? start : part === 'end' ? end : currentTime;
    const next = {
      ArrowLeft: value - step,
      ArrowDown: value - step,
      ArrowRight: value + step,
      ArrowUp: value + step,
      Home: 0,
      End: total,
    }[event.key];
    if (next === undefined) return;
    event.preventDefault();
    if (part === 'start') applyStart(next);
    else if (part === 'end') applyEnd(next);
    else onSeek(Math.min(total, Math.max(0, next)));
  };

  const slider = (part: 'start' | 'end' | 'playhead') => ({
    role: 'slider',
    tabIndex: disabled ? -1 : 0,
    'aria-disabled': disabled || undefined,
    'aria-valuemin': 0,
    'aria-valuemax': Number(total.toFixed(2)),
    'aria-valuenow': Number(
      (part === 'start' ? start : part === 'end' ? end : currentTime).toFixed(2)
    ),
    'aria-valuetext': formatClipTime(
      part === 'start' ? start : part === 'end' ? end : currentTime
    ),
    'data-part': part,
    onKeyDown: (event: KeyboardEvent<HTMLDivElement>) => onKeyDown(event, part),
  });

  /**
   * One cell per thumbnail of a strip covering `length` seconds from `from`, if in view. The
   * thumbnail tiles at its own aspect, as on the board, so a cell that zoom has stretched repeats
   * the frame instead of blowing it up.
   */
  const cells = (frames: string[], from: number, length: number) =>
    frames.flatMap((frame, index) => {
      const cellStart = from + (length * index) / frames.length;
      const cellEnd = from + (length * (index + 1)) / frames.length;
      if (cellEnd < view.start || cellStart > viewEnd) return [];
      return [
        <div
          key={index}
          className='absolute inset-y-0 bg-[length:auto_100%] bg-center bg-repeat-x'
          style={{
            left: `${share(cellStart)}%`,
            width: `${share(cellEnd) - share(cellStart)}%`,
            backgroundImage: `url("${frame}")`,
          }}
        />,
      ];
    });

  const handle =
    'absolute inset-y-0 z-20 grid w-2 place-items-center bg-accent outline-none before:absolute before:-inset-x-1.5 before:inset-y-0 after:h-[18px] after:w-0.5 after:rounded-sm after:bg-black/50 focus-visible:ring-2 focus-visible:ring-fg';
  const playheadInView = currentTime >= view.start && currentTime <= viewEnd;

  return (
    <div
      ref={surface}
      role='group'
      aria-label='Trim timeline'
      className={`relative touch-none select-none ${disabled ? 'cursor-not-allowed' : ''}`}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onLostPointerCapture={endDrag}
    >
      <div
        data-part='ruler'
        data-testid='timeline-ruler'
        data-step={ticks.step}
        data-minor-step={ticks.minorStep ?? undefined}
        aria-hidden
        className={`relative h-5 border-b border-line ${disabled ? '' : 'cursor-col-resize'}`}
      >
        {ticks.minor.map((time) => (
          <span
            key={`m${time}`}
            data-tick='minor'
            data-time={time}
            className='absolute bottom-0 h-1 w-px bg-faint'
            style={{ left: `${share(time)}%` }}
          />
        ))}
        {ticks.labelled.map(({ time, label }) => (
          <span key={`l${time}`} data-tick='labelled' data-time={time}>
            <span
              className='absolute bottom-0 h-1.5 w-px bg-dim'
              style={{ left: `${share(time)}%` }}
            />
            <span
              className='absolute top-[3px] -translate-x-1/2 whitespace-nowrap font-mono text-[10px] leading-none text-muted'
              style={{ left: `${share(time)}%` }}
            >
              {label}
            </span>
          </span>
        ))}
      </div>
      <div
        className={`relative mt-1.5 h-[72px] overflow-hidden rounded-md bg-black ${disabled ? '' : 'cursor-pointer'}`}
      >
        {thumbnails.length > 0 && (
          <div
            aria-hidden
            data-testid='timeline-thumbnails'
            className='pointer-events-none absolute inset-0 opacity-90'
          >
            {cells(thumbnails, 0, total)}
          </div>
        )}
        {rangeStrip && (
          <div
            aria-hidden
            data-testid='timeline-range-thumbnails'
            className='pointer-events-none absolute inset-0 opacity-90'
          >
            {cells(
              rangeStrip.frames,
              rangeStrip.range.start,
              rangeStrip.range.end - rangeStrip.range.start
            )}
          </div>
        )}
        <div
          className='pointer-events-none absolute inset-y-0 left-0 bg-[rgba(8,8,10,.62)]'
          style={{ width: `${clamp(share(start), 0, 100)}%` }}
        />
        <div
          className='pointer-events-none absolute inset-y-0 right-0 bg-[rgba(8,8,10,.62)]'
          style={{ width: `${clamp(100 - share(end), 0, 100)}%` }}
        />
        <div
          data-part='span'
          className={`absolute inset-y-0 rounded border-2 border-accent ${disabled ? '' : 'cursor-grab active:cursor-grabbing'}`}
          style={{
            left: place(start),
            width: `${clamp(share(end), -offTrack, 100 + offTrack) - clamp(share(start), -offTrack, 100 + offTrack)}%`,
          }}
        />
        <div
          {...slider('start')}
          aria-label='In point'
          aria-valuemax={Number(Math.max(0, end - shortest).toFixed(2))}
          className={`${handle} rounded-l ${disabled ? '' : 'cursor-ew-resize'}`}
          style={{ left: place(start) }}
        />
        <div
          {...slider('end')}
          aria-label='Out point'
          aria-valuemin={Number(Math.min(total, start + shortest).toFixed(2))}
          className={`${handle} -translate-x-full rounded-r ${disabled ? '' : 'cursor-ew-resize'}`}
          style={{ left: place(end) }}
        />
      </div>
      <div
        className={`pointer-events-none absolute bottom-0 top-0 z-30 -ml-px w-0.5 bg-white shadow-[0_0_0_1px_rgb(0_0_0/40%)] ${playheadInView ? '' : 'invisible'}`}
        style={{ left: place(currentTime) }}
      >
        <div
          {...slider('playhead')}
          aria-label='Playhead'
          className={`pointer-events-auto absolute -left-[5px] top-0 h-2.5 w-3 rounded-b-md rounded-t-sm bg-white shadow outline-none focus-visible:ring-2 focus-visible:ring-accent ${disabled ? '' : 'cursor-grab active:cursor-grabbing'}`}
        />
      </div>
    </div>
  );
}
