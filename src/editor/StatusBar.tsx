import type { MemoryEstimate } from '../ipc/export';
import type { Tool } from '../ipc/probe';
import type { Preview } from './useSource';
import { formatBytes } from './lib/output-settings';

/**
 * The window's bottom row: the ffmpeg and gifski the startup check found, what the player plays,
 * the frame-memory figure, and the shortcuts overlay's button. gifski is optional, so a missing
 * one is not mentioned; one on PATH that does not run gets a warning dot and its reason.
 */
export default function StatusBar({
  ffmpeg,
  gifski,
  preview,
  memory,
  onShowShortcuts,
}: {
  ffmpeg: Tool;
  gifski: Tool;
  /** The open source's preview; null when nothing is open. */
  preview: Preview | null;
  memory: MemoryEstimate | null;
  onShowShortcuts: () => void;
}) {
  return (
    <footer className='flex min-w-0 items-center gap-4 overflow-hidden whitespace-nowrap border-t border-line-subtle bg-chrome px-3 text-[11.5px] text-dim'>
      {ffmpeg.status === 'found' && (
        <ToolStatus name='ffmpeg' tool={ffmpeg} />
      )}
      {gifski.status !== 'missing' && (
        <ToolStatus name='gifski' tool={gifski} />
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

/** A tool on PATH: a green dot and its version, or a warning dot when it would not run. */
function ToolStatus({
  name,
  tool,
}: {
  name: string;
  tool: Exclude<Tool, { status: 'missing' }>;
}) {
  const found = tool.status === 'found';
  return (
    <span
      className='min-w-0 truncate'
      title={found ? tool.path : `${tool.path}: ${tool.reason}`}
    >
      <span
        aria-hidden
        className={`mr-1.5 inline-block h-[7px] w-[7px] rounded-full ${found ? 'bg-ok' : 'bg-warn'}`}
      />
      {found ? `${name} ${tool.version} on PATH` : `${name} on PATH does not run`}
    </span>
  );
}

function previewText(preview: Preview): string {
  if (preview.stage === 'building') return 'Making a preview copy';
  return preview.kind === 'source'
    ? 'Playing the source directly'
    : 'Playing a preview copy';
}
