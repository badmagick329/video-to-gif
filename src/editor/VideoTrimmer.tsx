import clsx from 'clsx';
import {
  ReactNode,
  RefObject,
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react';
import { ClipCrop, ClipTrim } from './lib/types';
import type { Size } from './lib/output-preview';
import {
  formatClipTime,
  parseClipTime,
  setSelectionEnd,
  setSelectionStart,
  toClipTrim,
} from './lib/trim';
import { useTimelineThumbnails } from './useTimelineThumbnails';
import { useClipShortcuts } from './useClipShortcuts';
import ClipTransport from './ClipTransport';
import TrimTimeline from './TrimTimeline';
import VideoCropper, { CropControls, cropSize } from './VideoCropper';
import {
  PaneLabel,
  ViewerHeader,
  ViewerMode,
  fitAspect,
  paneLabelHeight,
  useElementSize,
} from './Viewer';

type Playback = { start: number; end: number; loop: boolean };

/**
 * Selection end is treated as reached slightly early, so a loop restarts
 * before the browser paints a frame past the end.
 */
const endTolerance = 0.02;

/** Space between the panes in Side by side, in CSS pixels. */
const paneGap = 12;

/** The stage's size before it is measured, and in jsdom, which has no layout. */
const unmeasuredStage: Size = { width: 960, height: 540 };

/** Keeps preview playback inside the selection: loop back to the start, or stop at the end. */
function keepInSelection(player: HTMLVideoElement, playback: Playback) {
  const time = player.currentTime;
  const pastEnd = time >= playback.end - endTolerance;
  if (playback.loop) {
    if (pastEnd || time < playback.start - 0.3)
      player.currentTime = playback.start;
  } else if (pastEnd) {
    player.pause();
    player.currentTime = playback.end;
  }
}

const button =
  'inline-flex h-7 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-md px-2 text-md text-fg-soft hover:bg-hover hover:text-fg disabled:cursor-not-allowed disabled:opacity-50';

/**
 * What the trimmer needs to know about the source. It comes from the probe,
 * not the player, because the player may be showing a smaller proxy.
 */
export type TrimmerSource = {
  /** The source's path, which thumbnails are asked for by. */
  path: string;
  /** The preview source's URL: the source itself or its proxy. */
  src: string;
  duration: number;
  /** Displayed size; crop readouts are in these pixels. */
  width: number;
  height: number;
  frameRate: number;
};

/**
 * The editor's left column: the viewer (mode switch, crop controls, the stage and the transport)
 * over the timeline area. It owns the one `<video>`, which plays the preview source natively; the
 * output preview draws from it, so in Output mode the source pane stays mounted, clipped to a
 * pixel, rather than unmounting the video.
 */
export default function VideoTrimmer({
  source,
  trim,
  crop,
  disabled,
  active = true,
  card,
  mode,
  onModeChange,
  output,
  onChange,
  onCropChange,
  onMediaLoaded,
  onUnplayable,
  playbackRate = 1,
  sideways = false,
}: {
  source: TrimmerSource;
  trim?: ClipTrim;
  crop?: ClipCrop;
  disabled: boolean;
  /** Whether keyboard shortcuts act on this clip. */
  active?: boolean;
  /** The editor; keys pressed inside another element are not this clip's. */
  card?: RefObject<Element>;
  mode: ViewerMode;
  onModeChange: (mode: ViewerMode) => void;
  /** The output pane, given the stage area it may fill. */
  output: (area: Size) => ReactNode;
  onChange: (trim: ClipTrim | undefined) => void;
  onCropChange: (crop: ClipCrop | undefined) => void;
  /** The output preview draws from this element once it has a frame. */
  onMediaLoaded?: (video: HTMLVideoElement) => void;
  /** The player cannot decode the preview source, or shows no picture. */
  onUnplayable: () => void;
  /**
   * The output's speed. The editor plays at it, as Gif Studio's does, so
   * the output preview drawn from this video moves at the output's pace.
   */
  playbackRate?: number;
  /** The output is turned a quarter; see `VideoCropper`. */
  sideways?: boolean;
}) {
  const { src, duration: total } = source;
  const video = useRef<HTMLVideoElement | null>(null);
  // The playback rate effect needs the element as state to run when it mounts.
  const [player, setPlayer] = useState<HTMLVideoElement | null>(null);
  const attachVideo = useCallback((element: HTMLVideoElement | null) => {
    video.current = element;
    setPlayer(element);
  }, []);
  const [playError, setPlayError] = useState('');
  const [loopSelection, setLoopSelection] = useState(true);
  const [sound, setSound] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [ratio, setRatio] = useState<number | null>(null);
  const resumeAfterScrub = useRef(false);
  const stageRef = useRef<HTMLDivElement>(null);
  const stage = useElementSize(stageRef, unmeasuredStage);
  // The crop box works in the source's displayed pixels, which a proxy only
  // matches in aspect.
  const dimensions = { width: source.width, height: source.height };
  const sourceAspect = source.width / source.height;

  const start = trim?.start ?? 0;
  const end = trim?.end ?? total;
  // The probe knows the frame rate, where mgck has to measure it in playback.
  const frameSeconds = 1 / source.frameRate;
  const playback = useRef<Playback>({ start, end, loop: loopSelection });
  useEffect(() => {
    playback.current = { start, end, loop: loopSelection };
  });
  const thumbnails = useTimelineThumbnails(source.path, src);

  // Loading a source resets the rate to the default, so set both.
  useEffect(() => {
    if (!player) return;
    player.defaultPlaybackRate = playbackRate;
    player.playbackRate = playbackRate;
  }, [player, playbackRate, src]);

  // timeupdate fires only a few times a second; a frame loop keeps the
  // playhead smooth and the loop point tight while playing.
  useEffect(() => {
    const player = video.current;
    if (!playing || !player) return;
    let frame = requestAnimationFrame(function tick() {
      keepInSelection(player, playback.current);
      setCurrentTime(player.currentTime);
      frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
  }, [playing]);

  const selection = { start, end };
  const length = end - start;
  const change = (next: { start: number; end: number } | undefined) =>
    onChange(next && toClipTrim(next, total));
  const seek = (seconds: number) => {
    const clamped = Math.min(total, Math.max(0, seconds));
    if (video.current) video.current.currentTime = clamped;
    setCurrentTime(clamped);
  };
  const play = async () => {
    setPlayError('');
    try {
      await video.current!.play();
    } catch {
      setPlayError('Could not play this clip. Try a different file.');
    }
  };
  const togglePlay = () => {
    const player = video.current!;
    if (playing) {
      player.pause();
      return;
    }
    if (player.currentTime < start - 0.05 || player.currentTime >= end - 0.05)
      seek(start);
    void play();
  };
  const commitStart = (seconds: number) => {
    const next = setSelectionStart(selection, seconds, total, frameSeconds);
    change(next);
    seek(next.start);
  };
  const commitEnd = (seconds: number) => {
    const next = setSelectionEnd(selection, seconds, total, frameSeconds);
    change(next);
    seek(next.end);
  };
  const step = (seconds: number) => {
    video.current!.pause();
    seek(video.current!.currentTime + seconds);
  };
  // The element's time is fresher than state, which the frame loop updates
  // only once per animation frame while playing.
  const playheadTime = () => video.current?.currentTime ?? currentTime;

  useClipShortcuts(
    {
      togglePlay,
      previousFrame: () => step(-frameSeconds),
      nextFrame: () => step(frameSeconds),
      backOneSecond: () => step(-1),
      forwardOneSecond: () => step(1),
      setStart: () => commitStart(playheadTime()),
      setEnd: () => commitEnd(playheadTime()),
      jumpToStart: () => seek(start),
      jumpToEnd: () => seek(end),
    },
    { enabled: active && !disabled, scope: card }
  );

  // Side by side halves the stage; each pane keeps its picture's aspect inside its half.
  const split = mode === 'split';
  const paneArea: Size = split
    ? {
        width: Math.max(1, (stage.width - paneGap) / 2),
        height: Math.max(1, stage.height - paneLabelHeight),
      }
    : stage;
  const shown = fitAspect(paneArea, sourceAspect);
  const outputArea: Size = split
    ? paneArea
    : {
        width: stage.width,
        height: Math.max(1, stage.height - paneLabelHeight),
      };
  const kept = cropSize(crop, dimensions, sideways);

  return (
    <div className='flex min-h-0 min-w-0 flex-col'>
      <section
        aria-label='Viewer'
        className='flex min-h-0 flex-1 flex-col bg-viewer'
      >
        <ViewerHeader mode={mode} onModeChange={onModeChange}>
          <CropControls
            dimensions={dimensions}
            crop={crop}
            ratio={ratio}
            disabled={disabled}
            sideways={sideways}
            onRatioChange={setRatio}
            onChange={onCropChange}
          />
        </ViewerHeader>
        <fieldset
          disabled={disabled}
          className='flex min-h-0 min-w-0 flex-1 flex-col'
        >
          <legend className='sr-only'>Playback</legend>
          <div
            ref={stageRef}
            className='relative flex min-h-0 flex-1 items-center justify-center gap-3 overflow-hidden bg-stage p-4'
          >
            <div
              className={clsx(
                mode === 'output'
                  ? // Clipped to one pixel rather than hidden: the output preview draws from
                    // this video's presented frames, which a video that isn't rendered stops
                    // presenting.
                    'pointer-events-none absolute left-0 top-0 h-px w-px overflow-hidden'
                  : 'flex min-w-0 flex-col gap-1.5'
              )}
              aria-hidden={mode === 'output' || undefined}
              style={{ width: mode === 'output' ? undefined : shown.width }}
            >
              {split && (
                <PaneLabel>
                  Source ·{' '}
                  {crop
                    ? `crop ${kept.width} × ${kept.height}`
                    : 'full frame, drag a corner to crop'}
                </PaneLabel>
              )}
              <div style={{ width: shown.width }}>
                <VideoCropper
                  dimensions={dimensions}
                  crop={crop}
                  ratio={ratio}
                  disabled={disabled}
                  onChange={onCropChange}
                  maxHeight={shown.height}
                  sideways={sideways}
                >
                  <video
                    ref={attachVideo}
                    src={src}
                    // The asset protocol is another origin; without CORS the output
                    // preview's canvas would be tainted and draw nothing.
                    crossOrigin='anonymous'
                    muted={!sound}
                    playsInline
                    preload='auto'
                    className='block w-full bg-black'
                    style={{ maxHeight: shown.height }}
                    onLoadedMetadata={() => {
                      const player = video.current!;
                      // A video track the player can't decode, beside audio it can,
                      // loads with no picture.
                      if (player.videoWidth === 0) onUnplayable();
                      else onMediaLoaded?.(player);
                    }}
                    onError={onUnplayable}
                    onPlay={() => setPlaying(true)}
                    onPause={() => setPlaying(false)}
                    onEnded={() => {
                      // A selection ending at the clip's end stops the video before the
                      // frame loop sees it, so looping restarts it here.
                      if (!loopSelection) return;
                      seek(start);
                      void play();
                    }}
                    onSeeked={() => setCurrentTime(video.current!.currentTime)}
                    onTimeUpdate={() => {
                      const player = video.current!;
                      if (playing) keepInSelection(player, playback.current);
                      setCurrentTime(player.currentTime);
                    }}
                  />
                </VideoCropper>
              </div>
            </div>
            {mode !== 'source' && output(outputArea)}
          </div>
          <ClipTransport
            playing={playing}
            currentTime={currentTime}
            total={total}
            loopSelection={loopSelection}
            sound={sound}
            onTogglePlay={togglePlay}
            onStep={(direction) => step(direction * frameSeconds)}
            onJump={(to) => seek(to === 'start' ? start : end)}
            onLoopSelectionChange={setLoopSelection}
            onSoundChange={setSound}
          />
        </fieldset>
      </section>
      <section
        aria-label='Timeline'
        className='flex h-[180px] shrink-0 flex-col border-t border-line bg-dock'
      >
        <fieldset disabled={disabled} className='flex min-w-0 flex-col'>
          <legend className='sr-only'>Selection</legend>
          <div className='flex h-[38px] items-center gap-2 overflow-hidden px-3'>
            <span className='shrink-0 text-sm font-semibold'>Selection</span>
            <TimeField label='Start' seconds={start} onCommit={commitStart} />
            <TimeField label='End' seconds={end} onCommit={commitEnd} />
            <p className='shrink-0 whitespace-nowrap text-sm text-dim'>
              Length{' '}
              <span className='font-mono text-base tabular-nums text-fg'>
                {length.toFixed(2)} s
              </span>
            </p>
            <span className='flex-1' />
            <button
              type='button'
              className={button}
              onClick={() => commitStart(currentTime)}
            >
              Set start <span aria-hidden className='kbd'>I</span>
            </button>
            <button
              type='button'
              className={button}
              onClick={() => commitEnd(currentTime)}
            >
              Set end <span aria-hidden className='kbd'>O</span>
            </button>
            <button
              type='button'
              className={button}
              disabled={!trim}
              onClick={() => change(undefined)}
            >
              Whole clip
            </button>
          </div>
          <TrimTimeline
            total={total}
            selection={selection}
            currentTime={currentTime}
            frameSeconds={frameSeconds}
            thumbnails={thumbnails}
            disabled={disabled}
            onSelectionChange={change}
            onSeek={seek}
            onScrubChange={(scrubbing) => {
              if (scrubbing) {
                resumeAfterScrub.current = playing;
                video.current!.pause();
              } else if (resumeAfterScrub.current) {
                resumeAfterScrub.current = false;
                void play();
              }
            }}
          />
          {playError && (
            <p role='alert' className='px-4 pt-2 text-sm text-danger'>
              {playError}
            </p>
          )}
        </fieldset>
      </section>
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
        className='h-full w-[5.5rem] bg-transparent pr-2 font-mono text-base normal-case text-fg focus:outline-none'
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
