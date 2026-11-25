import type { FfmpegTools, Tool } from './ipc/probe';

function problem(name: string, tool: Tool): string | null {
  switch (tool.status) {
    case 'found':
      return null;
    case 'missing':
      return `${name} was not found on PATH.`;
    case 'unusable':
      return `${name} at ${tool.path} cannot be used: ${tool.reason}`;
  }
}

/** One tool's line in the check: a green dot when found, red with the problem otherwise. */
function ToolLine({ name, tool }: { name: string; tool: Tool }) {
  const trouble = problem(name, tool);
  return (
    <li className='flex items-baseline gap-2'>
      <span
        aria-hidden
        className={`inline-block h-[7px] w-[7px] shrink-0 -translate-y-px rounded-full ${trouble ? 'bg-danger' : 'bg-ok'}`}
      />
      {trouble ? (
        <span className='text-fg'>{trouble}</span>
      ) : (
        <span className='text-dim'>
          {name} {tool.status === 'found' && tool.version} on PATH
        </span>
      )}
    </li>
  );
}

/**
 * Shown in place of the editor when ffmpeg or ffprobe cannot be used: the editor's window, dark
 * stage and status bar, with what is missing and how to install it in the middle.
 */
export default function SetupMessage({ tools }: { tools: FfmpegTools }) {
  return (
    <main className='grid h-dvh grid-rows-[minmax(0,1fr)_26px] overflow-hidden bg-stage'>
      <div className='flex min-h-0 items-center justify-center overflow-y-auto p-8'>
        <section
          aria-labelledby='setup-heading'
          className='flex w-[520px] max-w-full flex-col gap-4 rounded-xl border border-line bg-panel p-6'
        >
          <div className='flex items-center gap-3'>
            <svg
              aria-hidden
              width='28'
              height='28'
              viewBox='0 0 24 24'
              fill='none'
              stroke='rgb(var(--accent))'
              strokeWidth='1.8'
            >
              <rect x='3' y='5' width='18' height='14' rx='2' />
              <path d='M10 9l5 3-5 3z' />
            </svg>
            <h1 id='setup-heading' className='text-lg font-medium'>
              ffmpeg is needed
            </h1>
          </div>
          <ul className='flex flex-col gap-1.5 rounded-lg bg-well px-3.5 py-3 text-md'>
            <ToolLine name='ffmpeg' tool={tools.ffmpeg} />
            <ToolLine name='ffprobe' tool={tools.ffprobe} />
          </ul>
          <p className='text-muted'>
            video-to-gif runs your own ffmpeg. Install ffmpeg, which includes
            ffprobe, for example with:
          </p>
          <code className='rounded-md border border-line-control bg-chrome px-3 py-2 font-mono text-sm text-fg'>
            winget install Gyan.FFmpeg
          </code>
          <p className='text-muted'>
            Then make sure both <code className='font-mono'>ffmpeg</code> and{' '}
            <code className='font-mono'>ffprobe</code> are on your PATH, and
            restart video-to-gif.
          </p>
        </section>
      </div>
      <footer className='flex items-center gap-4 border-t border-line-subtle bg-chrome px-3 text-[11.5px] text-dim'>
        <span>
          <span
            aria-hidden
            className='mr-1.5 inline-block h-[7px] w-[7px] rounded-full bg-danger'
          />
          The editor opens once ffmpeg and ffprobe run
        </span>
      </footer>
    </main>
  );
}
