import type { MemoryEstimate } from '../ipc/export';
import type { Tool } from '../ipc/probe';
import type { Preview } from './useSource';
import { formatBytes } from './lib/advanced';

/**
 * The window's bottom row: the ffmpeg the startup check found, what the player plays, the
 * frame-memory figure, and the shortcuts overlay's button.
 */
export default function StatusBar({
  ffmpeg,
  preview,
  memory,
  onShowShortcuts,
}: {
  ffmpeg: Tool;
  /** The open source's preview; null when nothing is open. */
  preview: Preview | null;
  memory: MemoryEstimate | null;
  onShowShortcuts: () => void;
}) {
  return (
    <footer className='flex min-w-0 items-center gap-4 overflow-hidden whitespace-nowrap border-t border-line-subtle bg-chrome px-3 text-[11.5px] text-dim'>
      {ffmpeg.status === 'found' && (
        <span className='min-w-0 truncate' title={ffmpeg.path}>
          <span
            aria-hidden
            className='mr-1.5 inline-block h-[7px] w-[7px] rounded-full bg-ok'
          />
          ffmpeg {ffmpeg.version} on PATH
        </span>
      )}
      {preview && <span>{previewText(preview)}</span>}
      <span className='flex-1' />
      {memory && memory.heldBytes > 0 && memory.availableBytes !== null && (
        <span className={memory.fits ? undefined : 'text-warn'}>
          Frame memory ≈ {formatBytes(memory.heldBytes)} of{' '}
          {formatBytes(memory.availableBytes)} free
        </span>
      )}
      <button
        type='button'
        aria-keyshortcuts='?'
        className='inline-flex items-center gap-1.5 hover:text-fg'
        onClick={onShowShortcuts}
      >
        Shortcuts{' '}
        <span aria-hidden className='kbd'>
          ?
        </span>
      </button>
    </footer>
  );
}

function previewText(preview: Preview): string {
  if (preview.stage === 'building') return 'Making a preview copy';
  return preview.kind === 'source'
    ? 'Playing the source directly'
    : 'Playing a preview copy';
}
