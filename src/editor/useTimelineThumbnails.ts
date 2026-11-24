import { useEffect, useRef, useState } from 'react';

import { fileUrl, makeThumbnails, type TimeRange } from '../ipc/preview';
import type { TimelineView } from './lib/timeline';

/**
 * The timeline's whole-source thumbnails for the open source at `path`, as URLs the webview can
 * load, or none while ffmpeg makes them or when it fails: a plain bar reads better than a broken
 * strip.
 *
 * mgck draws them by seeking a hidden `<video>`, which took 20 s on a long 4K source here;
 * ffmpeg makes the whole strip at once (`make_thumbnails`). `src`, the preview source,
 * re-requests them when a proxy replaces the source, so the strip follows what the player shows.
 */
export function useTimelineThumbnails(path: string, src: string) {
  // Frames are kept with the request they answer, so a new one shows none until its own
  // arrive, without resetting state in the effect.
  const key = `${path}|${src}`;
  const [strip, setStrip] = useState<{ key: string; frames: string[] }>({
    key,
    frames: [],
  });

  useEffect(() => {
    let current = true;
    makeThumbnails(path).then(
      (files) => {
        if (current) setStrip({ key, frames: files.map(fileUrl) });
      },
      () => {
        // The timeline works without thumbnails; nothing to tell the user.
      }
    );
    return () => {
      current = false;
    };
  }, [path, key]);

  return strip.key === key ? strip.frames : [];
}

/** A strip of thumbnails covering `range` of the source. */
export type RangeStrip = { range: TimeRange; frames: string[] };

/**
 * Past this zoom the whole-source strip's thumbnails stretch to more than one and a half times
 * their width, so a strip of the view is asked for.
 */
const denserPast = 1.5;

/** How long the view must sit still before its strip is asked for, so a scroll asks once. */
export const rangeThumbnailDelay = 300;

/**
 * A denser strip for a zoomed `view`: once the view has been still for
 * [`rangeThumbnailDelay`] ms, `make_thumbnails` is asked for the view plus half a view either
 * side, so a short scroll stays covered. The last strip stays up while the next is made: each
 * thumbnail is drawn at its own time, so it is never in the wrong place, only coarser. Null when
 * the whole-source strip is dense enough, before the first strip arrives, and when ffmpeg fails.
 */
export function useRangeThumbnails(
  path: string,
  src: string,
  view: TimelineView,
  total: number
): RangeStrip | null {
  const key = `${path}|${src}`;
  const wanted = view.span * denserPast < total;
  const [strip, setStrip] = useState<(RangeStrip & { key: string }) | null>(
    null
  );
  const start = Math.max(0, view.start - view.span / 2);
  const end = Math.min(total, view.start + view.span * 1.5);

  // Numbers requests, so a slow strip never replaces a later one. A strip that arrives after the
  // view moved on is still shown: it is drawn at its own times, and the next one is coming.
  const asked = useRef(0);
  const shown = useRef(0);

  useEffect(() => {
    if (!wanted) return;
    const timer = setTimeout(() => {
      const range = { start, end };
      const request = ++asked.current;
      makeThumbnails(path, range).then(
        (files) => {
          if (request < shown.current) return;
          shown.current = request;
          setStrip({ key, range, frames: files.map(fileUrl) });
        },
        () => {
          // The whole-source strip stands in.
        }
      );
    }, rangeThumbnailDelay);
    return () => clearTimeout(timer);
  }, [path, key, wanted, start, end]);

  return wanted && strip?.key === key ? strip : null;
}
