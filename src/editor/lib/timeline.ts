/**
 * The timeline dock's view onto the source: which span of seconds the ruler and track show, how
 * zooming and scrolling move it, and where the ruler's ticks fall. Pure, so the dock's maths is
 * tested without layout.
 */
import { formatClipTime } from './trim';

/** The visible part of the source's timeline: `span` seconds from `start`. */
export type TimelineView = { start: number; span: number };

/**
 * Frames across the track at the deepest zoom. At the window's 1100 px minimum the track is
 * about 730 px wide, so a frame is still 15 px: wide enough to see single frames apart and to
 * place a handle on one with the mouse.
 */
export const deepestZoomFrames = 48;

/** Labelled ticks are at least this far apart, room for `m:ss.cc` in the ruler's 10 px type. */
const labelSpacing = 72;

/** Unlabelled ticks are at least this far apart, or left out. */
const minorSpacing = 8;

const clamp = (value: number, min: number, max: number) =>
  Math.min(max, Math.max(min, value));

/** The narrowest span the dock zooms to: `deepestZoomFrames` frames, or the whole clip. */
export function shortestSpan(total: number, frameSeconds: number) {
  return Math.min(total, deepestZoomFrames * frameSeconds);
}

export function wholeView(total: number): TimelineView {
  return { start: 0, span: total };
}

/** Keeps the span between the deepest zoom and the whole clip, and the view inside the clip. */
export function clampView(
  view: TimelineView,
  total: number,
  frameSeconds: number
): TimelineView {
  const span = clamp(view.span, shortestSpan(total, frameSeconds), total);
  return { start: clamp(view.start, 0, total - span), span };
}

export function isZoomed(view: TimelineView, total: number) {
  return view.span < total;
}

/**
 * The view at `span`, keeping `anchor` (the playhead) where it sits on the track. An anchor out
 * of view is brought to the middle instead, since there is no place on the track to keep.
 */
export function zoomTo(
  view: TimelineView,
  span: number,
  anchor: number,
  total: number,
  frameSeconds: number
): TimelineView {
  const next = clamp(span, shortestSpan(total, frameSeconds), total);
  const offset = anchor - view.start;
  const share =
    offset >= 0 && offset <= view.span && view.span > 0
      ? offset / view.span
      : 0.5;
  return clampView(
    { start: anchor - share * next, span: next },
    total,
    frameSeconds
  );
}

/**
 * The zoom slider's position for a span, from 0 (whole clip) to 1 (deepest), on a log scale so
 * each step of the slider zooms by the same factor, on a 10 s clip or a 2 h one.
 */
export function zoomLevel(span: number, total: number, frameSeconds: number) {
  const shortest = shortestSpan(total, frameSeconds);
  if (shortest >= total) return 0;
  return clamp(Math.log(total / span) / Math.log(total / shortest), 0, 1);
}

export function spanAtLevel(
  level: number,
  total: number,
  frameSeconds: number
) {
  const shortest = shortestSpan(total, frameSeconds);
  return total * Math.pow(shortest / total, clamp(level, 0, 1));
}

/** Moves the view by `seconds`, stopping at the clip's edges. */
export function panBy(
  view: TimelineView,
  seconds: number,
  total: number,
  frameSeconds: number
): TimelineView {
  return clampView(
    { start: view.start + seconds, span: view.span },
    total,
    frameSeconds
  );
}

/**
 * The view that shows `time`: the same view when it already does, otherwise the page holding it,
 * with the time a tenth of the way in, as playback runs off the track's right edge.
 */
export function showTime(
  view: TimelineView,
  time: number,
  total: number,
  frameSeconds: number
): TimelineView {
  if (time >= view.start && time <= view.start + view.span) return view;
  return clampView(
    { start: time - view.span * 0.1, span: view.span },
    total,
    frameSeconds
  );
}

export type RulerTicks = {
  /** Seconds between labelled ticks. */
  step: number;
  labelled: { time: number; label: string }[];
  /** Seconds between unlabelled ticks, or null when there are none. */
  minorStep: number | null;
  minor: number[];
};

/** Tick intervals in seconds, finest first: whole frames below a second, then round times. */
function tickSteps(frameSeconds: number) {
  const frames = [1, 2, 5, 10, 15]
    .map((count) => count * frameSeconds)
    .filter((step) => step < 1);
  return [
    ...frames,
    1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200, 14400,
  ];
}

/** Whether `step` divides `outer` into whole parts. */
function divides(step: number, outer: number) {
  const parts = outer / step;
  return Math.abs(parts - Math.round(parts)) < 1e-6;
}

/** `m:ss` (or `h:mm:ss` from an hour) for whole-second ticks, `m:ss.cc` for finer ones. */
export function formatRulerTime(seconds: number, step: number) {
  if (step < 1) return formatClipTime(seconds);
  const whole = Math.round(seconds);
  const hours = Math.floor(whole / 3600);
  const minutes = Math.floor((whole % 3600) / 60);
  const secs = String(whole % 60).padStart(2, '0');
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, '0')}:${secs}`
    : `${minutes}:${secs}`;
}

/**
 * The ruler's ticks across `width` pixels of `view`: labelled ones at the finest interval that
 * leaves room for a label, and unlabelled ones at the finest interval that divides it and stays
 * visible. At the deepest zoom the unlabelled ticks are one frame apart.
 */
export function rulerTicks(
  view: TimelineView,
  width: number,
  frameSeconds: number
): RulerTicks {
  const perSecond = width / view.span;
  const steps = tickSteps(frameSeconds);
  const step =
    steps.find((candidate) => candidate * perSecond >= labelSpacing) ??
    steps.at(-1)!;
  const minorStep =
    steps.find(
      (candidate) =>
        candidate < step &&
        candidate * perSecond >= minorSpacing &&
        divides(candidate, step)
    ) ?? null;
  /** The multiples of `interval` in view, as their indices. */
  const indices = (interval: number) => {
    const first = Math.ceil(view.start / interval - 1e-9);
    const last = Math.floor((view.start + view.span) / interval + 1e-9);
    const out: number[] = [];
    for (let index = first; index <= last; index++) out.push(index);
    return out;
  };
  // Unlabelled ticks under a labelled one are left out by index, which float times can't do.
  const perLabel = minorStep === null ? 0 : Math.round(step / minorStep);
  return {
    step,
    labelled: indices(step).map((index) => ({
      time: index * step,
      label: formatRulerTime(index * step, step),
    })),
    minorStep,
    minor:
      minorStep === null
        ? []
        : indices(minorStep)
            .filter((index) => index % perLabel !== 0)
            .map((index) => index * minorStep),
  };
}
