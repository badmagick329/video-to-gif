import { getCurrentWebview } from '@tauri-apps/api/webview';
import { open } from '@tauri-apps/plugin-dialog';
import { useCallback, useEffect, useRef, useState } from 'react';

import {
  fileUrl,
  isPreviewError,
  makeProxy,
  onProxyProgress,
  openSource,
  type PreviewSource,
} from './preview';
import { isProbeError, probeSource, type SourceInfo } from './probe';

type View =
  | { stage: 'empty' }
  | { stage: 'probing'; path: string }
  | { stage: 'failed'; path: string; message: string }
  | { stage: 'buildingProxy'; path: string; info: SourceInfo; progress: number }
  | {
      stage: 'playing';
      path: string;
      info: SourceInfo;
      kind: PreviewSource;
      src: string;
    };

/** Extensions offered first in the file dialog; "All files" follows, since ffmpeg reads more. */
const VIDEO_EXTENSIONS = [
  'mp4',
  'mkv',
  'webm',
  'mov',
  'avi',
  'm4v',
  'ts',
  'mts',
  'm2ts',
  'flv',
  'wmv',
  'mpg',
  'mpeg',
  '3gp',
  'ogv',
  'gif',
];

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

function probeMessage(error: unknown, path: string): string {
  const name = fileName(path);
  if (!isProbeError(error)) return `Could not open ${name}: ${String(error)}`;
  switch (error.kind) {
    case 'ffprobeMissing':
      return 'ffprobe was not found on PATH, so the video cannot be read.';
    case 'ffprobeFailed':
      return `ffprobe could not start: ${error.reason}`;
    case 'notFound':
      return `There is no file at ${path}.`;
    case 'unreadable':
      return `ffmpeg cannot read ${name}: ${error.reason}`;
    case 'noVideoStream':
      return `${name} has no video in it.`;
    case 'invalidOutput':
      return `ffprobe described ${name} in a way the app does not understand: ${error.reason}`;
  }
}

function previewMessage(error: unknown, path: string): string {
  const name = fileName(path);
  if (!isPreviewError(error)) return `Could not preview ${name}: ${String(error)}`;
  switch (error.kind) {
    case 'notFound':
      return `There is no file at ${path}.`;
    case 'ffmpegMissing':
      return 'ffmpeg was not found on PATH, so no preview copy can be made.';
    case 'proxyFailed':
      return `ffmpeg could not make a preview copy of ${name}: ${error.reason}`;
    case 'notServable':
      return `The player cannot read ${name}: ${error.reason}`;
    default:
      return `Could not preview ${name} (${error.kind}).`;
  }
}

/**
 * Opens a source, by drop onto the window or the Open video button, and plays its preview
 * source: the file itself when WebView2 can play it, otherwise a proxy built by ffmpeg.
 */
export default function SourcePreview() {
  const [view, setView] = useState<View>({ stage: 'empty' });
  const [dragging, setDragging] = useState(false);
  // Each open gets a number; results for an older open are dropped, since the backend has already
  // replaced that source (and cancelled its proxy).
  const openCount = useRef(0);

  const buildProxy = useCallback(
    async (id: number, path: string, info: SourceInfo) => {
      const current = () => id === openCount.current;
      setView({ stage: 'buildingProxy', path, info, progress: 0 });
      try {
        const proxy = await makeProxy(path);
        if (current())
          setView({
            stage: 'playing',
            path,
            info,
            kind: 'proxy',
            src: fileUrl(proxy),
          });
      } catch (error) {
        if (!current()) return;
        if (isPreviewError(error) && error.kind === 'cancelled') return;
        setView({ stage: 'failed', path, message: previewMessage(error, path) });
      }
    },
    []
  );

  const openPath = useCallback(
    async (path: string) => {
      const id = ++openCount.current;
      const current = () => id === openCount.current;
      setView({ stage: 'probing', path });

      let info: SourceInfo;
      try {
        info = await probeSource(path);
      } catch (error) {
        if (current())
          setView({ stage: 'failed', path, message: probeMessage(error, path) });
        return;
      }
      if (!current()) return;

      let choice: PreviewSource;
      try {
        choice = await openSource(path, info);
      } catch (error) {
        if (current())
          setView({ stage: 'failed', path, message: previewMessage(error, path) });
        return;
      }
      if (!current()) return;

      if (choice === 'source') {
        setView({ stage: 'playing', path, info, kind: 'source', src: fileUrl(path) });
      } else {
        await buildProxy(id, path, info);
      }
    },
    [buildProxy]
  );

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    const keep = (unlisten: () => void) => {
      if (disposed) unlisten();
      else unlisteners.push(unlisten);
    };
    void onProxyProgress(({ source, progress }) => {
      setView((v) =>
        v.stage === 'buildingProxy' && v.path === source
          ? { ...v, progress }
          : v
      );
    }).then(keep);
    void getCurrentWebview()
      .onDragDropEvent(({ payload }) => {
        if (payload.type === 'enter' || payload.type === 'over') {
          setDragging(true);
        } else if (payload.type === 'leave') {
          setDragging(false);
        } else if (payload.type === 'drop') {
          setDragging(false);
          // Several files dropped at once: open the first, as the dialog opens one.
          const [first] = payload.paths;
          if (first !== undefined) void openPath(first);
        }
      })
      .then(keep);
    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [openPath]);

  const chooseFile = async () => {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [
        { name: 'Videos', extensions: VIDEO_EXTENSIONS },
        { name: 'All files', extensions: ['*'] },
      ],
    });
    if (typeof path === 'string') await openPath(path);
  };

  // The player could not decode the source after all (the allowlist admits it, WebView2 does not):
  // fall back to a proxy. A video it cannot decode but whose audio plays shows as zero width.
  const sourceUnplayable = () => {
    if (view.stage === 'playing' && view.kind === 'source')
      void buildProxy(openCount.current, view.path, view.info);
    else if (view.stage === 'playing')
      setView({
        stage: 'failed',
        path: view.path,
        message: `The preview copy of ${fileName(view.path)} could not play.`,
      });
  };

  return (
    <main
      className={`flex min-h-screen flex-col bg-zinc-950 text-zinc-100 ${
        dragging ? 'ring-4 ring-inset ring-sky-500' : ''
      }`}
    >
      <header className='flex items-center justify-between gap-4 border-b border-zinc-800 px-6 py-3'>
        <h1 className='text-xl font-semibold tracking-tight'>video-to-gif</h1>
        <button
          type='button'
          onClick={() => void chooseFile()}
          className='rounded bg-sky-600 px-4 py-1.5 text-sm font-medium hover:bg-sky-500'
        >
          Open video…
        </button>
      </header>
      <section className='flex flex-1 flex-col items-center justify-center gap-4 p-6'>
        <Body view={view} onUnplayable={sourceUnplayable} />
      </section>
    </main>
  );
}

function Body({
  view,
  onUnplayable,
}: {
  view: View;
  onUnplayable: () => void;
}) {
  switch (view.stage) {
    case 'empty':
      return (
        <p className='text-zinc-400'>
          Drop a video here, or choose Open video.
        </p>
      );
    case 'probing':
      return (
        <p className='text-zinc-300' aria-busy='true'>
          Opening {fileName(view.path)}…
        </p>
      );
    case 'failed':
      return (
        <p role='alert' className='max-w-2xl text-amber-300'>
          {view.message}
        </p>
      );
    case 'buildingProxy': {
      const percent = Math.round(view.progress * 100);
      return (
        <div className='flex w-full max-w-md flex-col gap-2'>
          <p className='text-zinc-300'>
            Making a preview copy of {fileName(view.path)}: {percent}%
          </p>
          <div
            role='progressbar'
            aria-label='Preview copy progress'
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={percent}
            className='h-2 overflow-hidden rounded bg-zinc-800'
          >
            <div className='h-full bg-sky-500' style={{ width: `${percent}%` }} />
          </div>
        </div>
      );
    }
    case 'playing':
      return (
        <figure className='flex w-full flex-1 flex-col items-center gap-2'>
          <video
            key={view.src}
            src={view.src}
            controls
            className='max-h-[70vh] w-full max-w-5xl bg-black'
            onError={onUnplayable}
            onLoadedMetadata={(event) => {
              if (event.currentTarget.videoWidth === 0) onUnplayable();
            }}
          />
          <figcaption className='text-sm text-zinc-400'>
            {fileName(view.path)}
            {view.kind === 'proxy' ? ' (preview copy)' : ''}
          </figcaption>
        </figure>
      );
  }
}
