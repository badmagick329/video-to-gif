import type { FfmpegTools, Tool } from './ipc/probe';

function problem(name: string, tool: Tool): string | null {
  switch (tool.status) {
    case 'found':
      return null;
    case 'missing':
      return `${name} was not found on PATH.`;
    case 'unusable':
      return `${name} at ${tool.path} did not run: ${tool.reason}`;
  }
}

/** Shown in place of the app when ffmpeg or ffprobe cannot be used. */
export default function SetupMessage({ tools }: { tools: FfmpegTools }) {
  const problems = [
    problem('ffmpeg', tools.ffmpeg),
    problem('ffprobe', tools.ffprobe),
  ].filter((p): p is string => p !== null);

  return (
    <main className='flex min-h-screen items-center justify-center bg-background-dg p-8 text-foreground-dg'>
      <section
        aria-labelledby='setup-heading'
        className='flex max-w-xl flex-col gap-4 rounded-md bg-secondary-dg p-6 shadow-glow-secondary-dg'
      >
        <h1 id='setup-heading' className='text-2xl font-semibold'>
          ffmpeg is needed
        </h1>
        <ul className='list-disc pl-5 text-sm text-amber-200'>
          {problems.map((p) => (
            <li key={p}>{p}</li>
          ))}
        </ul>
        <p className='text-sm text-foreground-dg/80'>
          video-to-gif runs your own ffmpeg. Install ffmpeg, which includes
          ffprobe (for example{' '}
          <code className='rounded bg-background-dg px-1'>
            winget install Gyan.FFmpeg
          </code>
          ), make sure both <code>ffmpeg</code> and <code>ffprobe</code> are on
          your PATH, then restart video-to-gif.
        </p>
      </section>
    </main>
  );
}
