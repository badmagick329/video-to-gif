import { LuFolderOpen, LuX } from 'react-icons/lu';

import type { SourceInfo } from '../ipc/probe';
import { fileName } from './useSource';
import { formatClipTime } from './lib/trim';

/** ffprobe's codec names as people know them; anything else is shown upper-cased. */
const codecNames: Record<string, string> = {
  h264: 'H.264',
  hevc: 'HEVC',
  av1: 'AV1',
  vp9: 'VP9',
  vp8: 'VP8',
  mpeg4: 'MPEG-4',
  mpeg2video: 'MPEG-2',
  prores: 'ProRes',
  aac: 'AAC',
  mp3: 'MP3',
  opus: 'Opus',
  vorbis: 'Vorbis',
  flac: 'FLAC',
  ac3: 'AC-3',
  eac3: 'E-AC-3',
  dts: 'DTS',
  alac: 'ALAC',
};

export function codecName(codec: string): string {
  if (codec.startsWith('pcm_')) return 'PCM';
  return codecNames[codec] ?? codec.toUpperCase();
}

/** What the probe says about the source, as the toolbar's chips. */
export function sourceChips(info: SourceInfo): string[] {
  return [
    `${info.width} × ${info.height}`,
    codecName(info.videoCodec),
    `${Number(info.frameRate.toFixed(2))} fps`,
    formatClipTime(info.duration),
    ...(info.audioCodec ? [codecName(info.audioCodec)] : []),
  ];
}

/**
 * The window's toolbar: Open, then the open source's name (full path in its tooltip) with the
 * probe's chips, and Close.
 */
export default function Toolbar({
  path,
  info,
  closeDisabled,
  onOpen,
  onClose,
}: {
  /** The open (or opening) source's path; null when nothing is open. */
  path: string | null;
  /** The open source's probe; null until it is read. */
  info: SourceInfo | null;
  closeDisabled: boolean;
  onOpen: () => void;
  onClose: () => void;
}) {
  const name = path && fileName(path);
  return (
    <header className='flex min-w-0 items-center gap-2.5 border-b border-line bg-panel px-3'>
      <button
        type='button'
        aria-keyshortcuts='Control+O'
        className='inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md border border-line-strong bg-control px-2.5 text-md hover:bg-control-hover'
        onClick={onOpen}
      >
        <LuFolderOpen aria-hidden className='h-[15px] w-[15px]' />
        Open
        <span aria-hidden className='kbd'>
          Ctrl O
        </span>
      </button>
      {name ? (
        <>
          <div aria-hidden className='h-5 w-px shrink-0 bg-line-control' />
          <p className='min-w-0 max-w-[460px] truncate font-medium' title={path}>
            {name}
          </p>
          {info && (
            <ul
              aria-label='Source details'
              className='flex min-w-0 shrink gap-1.5 overflow-hidden text-sm text-muted'
            >
              {sourceChips(info).map((chip) => (
                <li
                  key={chip}
                  className='shrink-0 whitespace-nowrap rounded bg-control px-[7px] py-0.5'
                >
                  {chip}
                </li>
              ))}
            </ul>
          )}
          <span className='flex-1' />
          <button
            type='button'
            aria-label={`Close ${name}`}
            className='inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md px-2.5 text-md text-fg-soft hover:bg-hover hover:text-fg disabled:cursor-not-allowed disabled:opacity-50'
            disabled={closeDisabled}
            onClick={onClose}
          >
            <LuX aria-hidden className='h-3.5 w-3.5' />
            Close
          </button>
        </>
      ) : (
        <p className='text-dim'>No video open</p>
      )}
    </header>
  );
}
