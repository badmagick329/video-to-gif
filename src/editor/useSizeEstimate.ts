import {
  useCallback,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';

import {
  cancelSizeEstimate,
  estimateOutputSize,
  type SizeEstimate,
} from '../ipc/export';
import { hasAudio, type SourceInfo } from '../ipc/probe';
import { OutputSettings, FileType, fileTypeOrder } from './lib/output-settings';
import { hasCaptions, renderCaptionImage } from './lib/captions';
import type { Size } from './lib/output-preview';
import type { ClipCrop, ClipTrim } from './lib/types';

/**
 * How long the selection, crop and settings must stay still before an estimate starts. Sliders
 * and trim handles change on every drag step, and each estimate is several ffmpeg runs.
 */
export const ESTIMATE_SETTLE_MS = 1500;

/** One ticked file type's figure, as the output preview shows it. */
export type FileTypeEstimate =
  /** No figure for anything like these settings yet. */
  | { state: 'estimating' }
  | { state: 'ready'; bytes: number }
  /** The figure for earlier settings, shown greyed until the new one lands. */
  | { state: 'stale'; bytes: number }
  | { state: 'failed'; reason: string };

export type EstimateInput = {
  path: string;
  info: SourceInfo;
  trim?: ClipTrim;
  crop?: ClipCrop;
  settings: OutputSettings;
  /** The exact output size, which the caption image is drawn at. */
  size: Size;
};

/**
 * What each file type's size depends on, as comparable strings. A change reruns only the file
 * types whose key changed: GIF colours or dithering only the GIF, WebP quality only the WebP,
 * keep sound only the MP4; anything else every ticked file type.
 */
export function estimateKeys(input: EstimateInput): Record<FileType, string> {
  const { path, info, trim, crop, settings } = input;
  const common = {
    path,
    duration: info.duration,
    trim,
    crop,
    width: settings.width,
    fps: settings.fps,
    speed: settings.speed,
    direction: settings.direction,
    rotation: settings.rotation,
    flipHorizontal: settings.flipHorizontal,
    flipVertical: settings.flipVertical,
    plays: settings.plays,
    topText: settings.topText,
    bottomText: settings.bottomText,
    captionSize: settings.captionSize,
  };
  return {
    gif: JSON.stringify([
      common,
      settings.gifColors,
      settings.gifDither,
    ]),
    webp: JSON.stringify([common, settings.webpQuality]),
    mp4: JSON.stringify([common, keepSound(settings, info)]),
  };
}

function keepSound(settings: OutputSettings, info: SourceInfo): boolean {
  return settings.keepSound && hasAudio(info);
}

type Outcome = { key: string } & ({ bytes: number } | { reason: string });

/**
 * Rust's size estimate for each ticked file type, scheduled as background work: it starts
 * `ESTIMATE_SETTLE_MS` after the last change to the selection, crop or settings, any change
 * kills the one running, and it waits while `paused` (the video playing or an export running).
 * Each run asks only for the ticked file types whose figure is missing or out of date.
 */
export function useSizeEstimate(
  input: EstimateInput,
  paused: boolean
): Partial<Record<FileType, FileTypeEstimate>> {
  const [outcomes, setOutcomes] = useState<Partial<Record<FileType, Outcome>>>(
    {}
  );
  const keys = estimateKeys(input);
  const ticked = fileTypeOrder.filter((type) =>
    input.settings.fileTypes.includes(type)
  );

  // The latest render's values, for the timer that fires after them. Declared before the
  // scheduling effect, so it is current whenever that effect runs.
  const latest = useRef({ input, keys, ticked, outcomes });
  useEffect(() => {
    latest.current = { input, keys, ticked, outcomes };
  });
  /** Bumped to disown a run: its late answers are dropped. */
  const generation = useRef(0);
  const running = useRef(false);

  const signature = JSON.stringify(ticked.map((type) => [type, keys[type]]));

  useEffect(() => {
    const stop = () => {
      generation.current += 1;
      if (running.current) {
        running.current = false;
        void cancelSizeEstimate().catch(() => {});
      }
    };
    if (paused) return stop;

    const start = async () => {
      const { input, keys, ticked, outcomes } = latest.current;
      const fileTypes = ticked.filter(
        (type) => outcomes[type]?.key !== keys[type]
      );
      if (fileTypes.length === 0) return;
      const run = ++generation.current;
      running.current = true;
      const record = (estimate: SizeEstimate) => {
        if (run !== generation.current || estimate.status === 'cancelled')
          return;
        const key = keys[estimate.fileType];
        setOutcomes((current) => ({
          ...current,
          [estimate.fileType]:
            estimate.status === 'done'
              ? { key, bytes: estimate.bytes }
              : { key, reason: estimate.reason },
        }));
      };
      try {
        const { path, info, trim, crop, settings, size } = input;
        const captions = hasCaptions(settings)
          ? ((await renderCaptionImage(settings, size)) ?? undefined)
          : undefined;
        if (run !== generation.current) return;
        const settled = await estimateOutputSize(
          {
            path,
            info,
            trim,
            crop,
            settings: { ...settings, fileTypes },
            keepSound: keepSound(settings, info),
            captions,
          },
          record
        );
        // The channel delivers each as it settles; this catches any it dropped.
        settled.forEach(record);
      } catch {
        // Rejected before any sample ran (an export started, or ffmpeg went missing): the
        // figures stay as they were and the next change tries again.
      } finally {
        if (run === generation.current) running.current = false;
      }
    };
    const timer = setTimeout(() => void start(), ESTIMATE_SETTLE_MS);
    return () => {
      clearTimeout(timer);
      stop();
    };
  }, [signature, paused]);

  const shown: Partial<Record<FileType, FileTypeEstimate>> = {};
  for (const type of ticked) {
    const outcome = outcomes[type];
    if (!outcome) shown[type] = { state: 'estimating' };
    else if (outcome.key === keys[type])
      shown[type] =
        'bytes' in outcome
          ? { state: 'ready', bytes: outcome.bytes }
          : { state: 'failed', reason: outcome.reason };
    else
      shown[type] =
        'bytes' in outcome
          ? { state: 'stale', bytes: outcome.bytes }
          : { state: 'estimating' };
  }
  return shown;
}

/**
 * Whether `video` is playing: estimates pause while it does, so the player keeps the CPU.
 */
export function usePlaying(video: HTMLVideoElement | null): boolean {
  const subscribe = useCallback(
    (onChange: () => void) => {
      if (!video) return () => {};
      playbackEvents.forEach((name) => video.addEventListener(name, onChange));
      return () =>
        playbackEvents.forEach((name) =>
          video.removeEventListener(name, onChange)
        );
    },
    [video]
  );
  return useSyncExternalStore(
    subscribe,
    () => video !== null && !video.paused && !video.ended
  );
}

const playbackEvents = ['play', 'playing', 'pause', 'ended', 'emptied'] as const;
