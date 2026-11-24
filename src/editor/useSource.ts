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
} from '../ipc/preview';
import { isProbeError, probeSource, type SourceInfo } from '../ipc/probe';

/** What the player has to play, once the source is open. */
export type Preview =
  | { stage: 'building'; progress: number }
  | { stage: 'ready'; kind: PreviewSource; src: string };

export type OpenSource = {
  /** Counts opens, so reopening the same path still gives a fresh editor. */
  id: number;
  path: string;
  info: SourceInfo;
  preview: Preview;
};

export type SourceState =
  | { stage: 'empty' }
  | { stage: 'probing'; path: string }
  | { stage: 'failed'; path: string; message: string }
  | ({ stage: 'open' } & OpenSource);

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

export function fileName(path: string): string {
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
  if (!isPreviewError(error))
    return `Could not preview ${name}: ${String(error)}`;
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
 * The editor's source: opened by a drop anywhere on the window or the file dialog, probed, and
 * given a preview source, the file itself when WebView2 can play it, otherwise a proxy built by
 * ffmpeg. This replaces mgck's dropzone and paste hooks, which hand over in-memory files.
 *
 * `onOpen` runs as each new source starts opening, so the editor can reset per-clip state.
 */
export function useSource(onOpen: () => void) {
  const [state, setState] = useState<SourceState>({ stage: 'empty' });
  const [dragging, setDragging] = useState(false);
  // Each open gets a number; results for an older open are dropped, since the backend has
  // already replaced that source (and cancelled its proxy).
  const openCount = useRef(0);
  const latestOnOpen = useRef(onOpen);
  useEffect(() => {
    latestOnOpen.current = onOpen;
  });

  const buildProxy = useCallback(
    async (id: number, path: string, info: SourceInfo) => {
      const current = () => id === openCount.current;
      setState({
        stage: 'open',
        id,
        path,
        info,
        preview: { stage: 'building', progress: 0 },
      });
      try {
        const proxy = await makeProxy(path);
        if (current())
          setState({
            stage: 'open',
            id,
            path,
            info,
            preview: { stage: 'ready', kind: 'proxy', src: fileUrl(proxy) },
          });
      } catch (error) {
        if (!current()) return;
        if (isPreviewError(error) && error.kind === 'cancelled') return;
        setState({
          stage: 'failed',
          path,
          message: previewMessage(error, path),
        });
      }
    },
    []
  );

  const openPath = useCallback(
    async (path: string) => {
      const id = ++openCount.current;
      const current = () => id === openCount.current;
      latestOnOpen.current();
      setState({ stage: 'probing', path });

      let info: SourceInfo;
      try {
        info = await probeSource(path);
      } catch (error) {
        if (current())
          setState({
            stage: 'failed',
            path,
            message: probeMessage(error, path),
          });
        return;
      }
      if (!current()) return;

      let choice: PreviewSource;
      try {
        choice = await openSource(path, info);
      } catch (error) {
        if (current())
          setState({
            stage: 'failed',
            path,
            message: previewMessage(error, path),
          });
        return;
      }
      if (!current()) return;

      if (choice === 'source') {
        setState({
          stage: 'open',
          id,
          path,
          info,
          preview: { stage: 'ready', kind: 'source', src: fileUrl(path) },
        });
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
      setState((s) =>
        s.stage === 'open' &&
        s.path === source &&
        s.preview.stage === 'building'
          ? { ...s, preview: { stage: 'building', progress } }
          : s
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

  const chooseFile = useCallback(async () => {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [
        { name: 'Videos', extensions: VIDEO_EXTENSIONS },
        { name: 'All files', extensions: ['*'] },
      ],
    });
    if (typeof path === 'string') await openPath(path);
  }, [openPath]);

  /** Leaves the editor empty. The backend keeps the source open until another replaces it. */
  const close = useCallback(() => {
    openCount.current++;
    setState({ stage: 'empty' });
  }, []);

  /**
   * The player could not decode the source after all (the allowlist admits it, WebView2 does
   * not): fall back to a proxy. A proxy that fails to play is reported instead.
   */
  const unplayable = useCallback(() => {
    if (state.stage !== 'open' || state.preview.stage !== 'ready') return;
    if (state.preview.kind === 'source')
      void buildProxy(state.id, state.path, state.info);
    else
      setState({
        stage: 'failed',
        path: state.path,
        message: `The preview copy of ${fileName(state.path)} could not play.`,
      });
  }, [state, buildProxy]);

  return { state, dragging, chooseFile, close, unplayable };
}
