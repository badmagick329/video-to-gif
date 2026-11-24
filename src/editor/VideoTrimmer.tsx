
import { RefObject, useCallback, useEffect, useRef, useState } from 'react';
import { ClipCrop, ClipTrim } from './lib/types';
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
import VideoCropper from './VideoCropper';

type Playback = { start: number; end: number; loop: boolean };

/**
 * Selection end is treated as reached slightly early, so a loop restarts
 * before the browser paints a frame past the end.
 */
const endTolerance = 0.02;

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
  'h-9 rounded-md border border-foreground-dg/25 px-3 text-sm hover:bg-foreground-dg/10 disabled:cursor-not-allowed disabled:opacity-50';

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

/** Native playback keeps editing responsive without generating encoded previews. */
export default function VideoTrimmer({
  source,
  trim,
  crop,
  disabled,
  active = true,
  card,
  onChange,
  onCropChange,
  onMediaLoaded,
  onUnplayable,
  videoMaxHeight = 256,
  playbackRate = 1,
  sideways = false,
}: {
  source: TrimmerSource;
  trim?: ClipTrim;
  crop?: ClipCrop;
  disabled: boolean;
  /** Whether keyboard shortcuts act on this clip. */
  active?: boolean;
  /** The clip's card; keys pressed inside another element are not this clip's. */
  card?: RefObject<Element>;
  onChange: (trim: ClipTrim | undefined) => void;
  onCropChange: (crop: ClipCrop | undefined) => void;
  /** The output preview draws from this element once it has a frame. */
  onMediaLoaded?: (video: HTMLVideoElement) => void;
  /** The player cannot decode the preview source, or shows no picture. */
  onUnplayable: () => void;
  /** Simple-mode cards are narrow; Advanced mode's editor column has room. */
  videoMaxHeight?: number;
  /**
   * Advanced mode's speed. The editor plays at it, as Gif Studio's does, so
   * the output preview drawn from this video moves at the output's pace.
   */
  playbackRate?: number;
  /** Advanced mode turned the output a quarter; see `VideoCropper`. */
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
  const resumeAfterScrub = useRef(false);
  // The crop box works in the source's displayed pixels, which a proxy only
  // matches in aspect.
  const dimensions = { width: source.width, height: source.height };

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

  return (
    <fieldset
      disabled={disabled}
      className='flex w-full min-w-0 flex-col gap-3 text-left disabled:opacity-60'
    >
      <legend className='sr-only'>Selection</legend>
      <VideoCropper
        dimensions={dimensions}
        crop={crop}
        disabled={disabled}
        onChange={onCropChange}
        maxHeight={videoMaxHeight}
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
          className='w-full rounded-md bg-black'
          style={{ maxHeight: videoMaxHeight }}
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
      <ClipTransport
        playing={playing}
        currentTime={currentTime}
        total={total}
        loopSelection={loopSelection}
        sound={sound}
        onTogglePlay={togglePlay}
        onStep={(direction) => step(direction * frameSeconds)}
        onLoopSelectionChange={setLoopSelection}
        onSoundChange={setSound}
      />
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
      <div className='flex flex-wrap items-center gap-x-4 gap-y-2 text-sm'>
        <TimeField label='Start' seconds={start} onCommit={commitStart} />
        <TimeField label='End' seconds={end} onCommit={commitEnd} />
        <p>
          Length{' '}
          <span className='font-semibold tabular-nums'>
            {length.toFixed(2)} s
          </span>
        </p>
      </div>
      <div className='flex flex-wrap gap-2'>
        <button
          type='button'
          className={button}
          onClick={() => commitStart(currentTime)}
        >
          Set start
        </button>
        <button
          type='button'
          className={button}
          onClick={() => commitEnd(currentTime)}
        >
          Set end
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
      <ShortcutHints />
      {playError && <p role='alert'>{playError}</p>}
    </fieldset>
  );
}

/** Keys as tokens; `+` and `/` are separators drawn as plain text. */
const shortcutHints: Array<[keys: string[], action: string]> = [
  [['Space'], 'play'],
  [['←', '→'], 'frame'],
  [['Shift', '+', '←', '→'], '1 s'],
  [['I', '/', 'O'], 'start/end'],
  [['Home', '/', 'End'], 'jump'],
  [['Ctrl', '+', 'Enter'], 'convert'],
  [['Ctrl', '+', 'O'], 'open'],
];
const separators = new Set(['+', '/']);

/**
 * Gif Studio's hint row. Hidden unless some pointer is fine, so touch-only
 * screens, which have no keyboard to use it with, don't spend space on it.
 */
function ShortcutHints() {
  return (
    <ul
      aria-label='Keyboard shortcuts'
      className='hidden flex-wrap gap-x-3 gap-y-1 text-xs text-foreground-dg/60 [@media(any-pointer:fine)]:flex'
    >
      {shortcutHints.map(([keys, action]) => (
        <li key={action} className='flex items-center gap-0.5'>
          {keys.map((key, index) =>
            separators.has(key) ? (
              <span key={index}>{key}</span>
            ) : (
              <kbd
                key={index}
                className='rounded border border-b-2 border-foreground-dg/25 bg-foreground-dg/10 px-1 font-mono text-[11px] leading-4 text-foreground-dg'
              >
                {key}
              </kbd>
            )
          )}{' '}
          <span className='ml-1'>{action}</span>
        </li>
      ))}
    </ul>
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
    <label className='flex items-center gap-1.5'>
      {label}
      <input
        type='text'
        inputMode='decimal'
        spellCheck={false}
        autoComplete='off'
        className='h-8 w-[5.5rem] rounded-md border border-foreground-dg/25 bg-background-dg px-2 font-mono text-[13px] focus:border-primary-dg focus:outline-none'
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
