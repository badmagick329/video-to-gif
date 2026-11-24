import {
  LuChevronLeft,
  LuChevronRight,
  LuPause,
  LuPlay,
  LuRepeat,
  LuSkipBack,
  LuSkipForward,
  LuVolume2,
  LuVolumeX,
} from 'react-icons/lu';
import { formatClipTime } from './lib/trim';

const iconButton =
  'grid h-[30px] w-[30px] place-items-center rounded-md text-fg-label hover:bg-hover hover:text-white aria-pressed:text-accent-soft disabled:cursor-not-allowed disabled:opacity-50';

/**
 * The viewer's transport row, under the stage. Replaces the native controls so the video surface
 * stays free for the crop box.
 */
export default function ClipTransport({
  playing,
  currentTime,
  total,
  loopSelection,
  sound,
  onTogglePlay,
  onStep,
  onJump,
  onLoopSelectionChange,
  onSoundChange,
}: {
  playing: boolean;
  currentTime: number;
  total: number;
  loopSelection: boolean;
  sound: boolean;
  onTogglePlay: () => void;
  onStep: (direction: -1 | 1) => void;
  /** Moves the playhead to the selection's in or out point. */
  onJump: (to: 'start' | 'end') => void;
  onLoopSelectionChange: (loop: boolean) => void;
  onSoundChange: (sound: boolean) => void;
}) {
  return (
    <div className='flex h-11 shrink-0 items-center gap-1.5 border-t border-line-subtle bg-dock px-3'>
      <button
        type='button'
        aria-label='Go to in point'
        className={iconButton}
        onClick={() => onJump('start')}
      >
        <LuSkipBack aria-hidden className='h-4 w-4' />
      </button>
      <button
        type='button'
        aria-label='Previous frame'
        className={iconButton}
        onClick={() => onStep(-1)}
      >
        <LuChevronLeft aria-hidden className='h-4 w-4' />
      </button>
      <button
        type='button'
        aria-label={playing ? 'Pause' : 'Play'}
        className='grid h-[34px] w-[34px] place-items-center rounded-full bg-fg text-[#111] hover:bg-white hover:text-black disabled:opacity-50'
        onClick={onTogglePlay}
      >
        {playing ? (
          <LuPause aria-hidden className='h-3.5 w-3.5 fill-current' />
        ) : (
          <LuPlay aria-hidden className='ml-0.5 h-3.5 w-3.5 fill-current' />
        )}
      </button>
      <button
        type='button'
        aria-label='Next frame'
        className={iconButton}
        onClick={() => onStep(1)}
      >
        <LuChevronRight aria-hidden className='h-4 w-4' />
      </button>
      <button
        type='button'
        aria-label='Go to out point'
        className={iconButton}
        onClick={() => onJump('end')}
      >
        <LuSkipForward aria-hidden className='h-4 w-4' />
      </button>
      <p className='ml-2.5 font-mono text-base tabular-nums'>
        {formatClipTime(currentTime)}{' '}
        <span className='text-dim'>/ {formatClipTime(total)}</span>
      </p>
      <span className='flex-1' />
      <button
        type='button'
        aria-label='Loop selection'
        title='Loop selection'
        aria-pressed={loopSelection}
        className={iconButton}
        onClick={() => onLoopSelectionChange(!loopSelection)}
      >
        <LuRepeat aria-hidden className='h-4 w-4' />
      </button>
      <button
        type='button'
        aria-label='Sound'
        title={sound ? 'Sound on' : 'Sound off'}
        aria-pressed={sound}
        className={iconButton}
        onClick={() => onSoundChange(!sound)}
      >
        {sound ? (
          <LuVolume2 aria-hidden className='h-4 w-4' />
        ) : (
          <LuVolumeX aria-hidden className='h-4 w-4' />
        )}
      </button>
    </div>
  );
}
