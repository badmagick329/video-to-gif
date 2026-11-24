import { useEffect, useState } from 'react';

import { findFfmpeg, type FfmpegTools } from './ipc/probe';
import Editor from './editor/Editor';
import SetupMessage from './SetupMessage';

type Startup =
  | { stage: 'checking' }
  | { stage: 'ready' }
  | { stage: 'setup'; tools: FfmpegTools }
  | { stage: 'failed'; message: string };

export default function App() {
  const [startup, setStartup] = useState<Startup>({ stage: 'checking' });

  useEffect(() => {
    let cancelled = false;
    findFfmpeg().then(
      (tools) => {
        if (cancelled) return;
        const ready =
          tools.ffmpeg.status === 'found' && tools.ffprobe.status === 'found';
        setStartup(ready ? { stage: 'ready' } : { stage: 'setup', tools });
      },
      (error: unknown) => {
        if (!cancelled) setStartup({ stage: 'failed', message: String(error) });
      }
    );
    return () => {
      cancelled = true;
    };
  }, []);

  switch (startup.stage) {
    case 'checking':
      return <main className='min-h-screen bg-canvas' aria-busy='true' />;
    case 'setup':
      return <SetupMessage tools={startup.tools} />;
    case 'failed':
      return (
        <main className='flex min-h-screen items-center justify-center bg-canvas p-8 text-fg'>
          <p role='alert'>Could not check for ffmpeg: {startup.message}</p>
        </main>
      );
    case 'ready':
      return <Editor />;
  }
}
