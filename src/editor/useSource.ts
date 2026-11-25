import { getCurrentWebview } from '@tauri-apps/api/webview';
import { open } from '@tauri-apps/plugin-dialog';
import { useCallback, useEffect, useRef, useState } from 'react';

import {
  closeSource,
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
  | { stage: 'failed'; path: string; failure: OpenFailure }
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

/**
 * Why a source did not open, in plain words, with the tool's own message folded under it: the
 * viewer shows `text` and keeps `detail` collapsed until asked.
 */
export type OpenFailure = {
  text: string;
  detail?: { label: string; text: string };
};

const ffmpegsReason = (text: string) => ({ label: "ffmpeg's reason", text });

function probeFailure(error: unknown, path: string): OpenFailure {
  const name = fileName(path);
  if (!isProbeError(error))
    return {
      text: `Could not open ${name}.`,
      detail: { label: 'Details', text: String(error) },
    };
  switch (error.kind) {
    case 'ffprobeMissing':
      return {
        text: 'ffprobe was not found on PATH, so the video cannot be read.',
      };
    case 'ffprobeFailed':
      return {
        text: `ffprobe could not start, so ${name} cannot be read.`,
        detail: ffmpegsReason(error.reason),
      };
    case 'notFound':
      return { text: `There is no file at ${path}.` };
    case 'unreadable':
      return {
        text: `ffmpeg cannot read ${name}.`,
        detail: ffmpegsReason(error.reason),
      };
    case 'noVideoStream':
      return { text: `${name} has no video in it.` };
    case 'invalidOutput':
      return {
        text: `ffprobe described ${name} in a way the app does not understand.`,
        detail: ffmpegsReason(error.reason),
      };
  }
}

function previewFailure(error: unknown, path: string): OpenFailure {
  const name = fileName(path);
  if (!isPreviewError(error))
    return {
      text: `Could not preview ${name}.`,
      detail: { label: 'Details', text: String(error) },
    };
  switch (error.kind) {
    case 'notFound':
      return { text: `There is no file at ${path}.` };
    case 'ffmpegMissing':
      return {
        text: 'ffmpeg was not found on PATH, so no preview copy can be made.',
      };
    case 'proxyFailed':
      return {
        text: `ffmpeg could not make a preview copy of ${name}.`,
        detail: ffmpegsReason(error.reason),
      };
    case 'notServable':
      return {
        text: `The player cannot read ${name}.`,
        detail: { label: 'Details', text: error.reason },
      };
    default:
      return {
        text: `Could not preview ${name}.`,
        detail: { label: 'Details', text: error.kind },
      };
  }
}

/**
 * The editor's source: opened by a drop anywhere on the window, the file dialog or Recent, probed,
 * and given a preview source, the file itself when WebView2 can play it, otherwise a proxy built
 * by ffmpeg. All three hand `openPath` a path, so each reaches Rust's allowlist the same way:
 * `probe_source`, then `open_source`, which admits the path and adds it to Recent. A drop with a
 * source open replaces it.
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
          failure: previewFailure(error, path),
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
            failure: probeFailure(error, path),
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
            failure: previewFailure(error, path),
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

  /**
   * Leaves the editor empty and closes the source in the backend, which stops a proxy build still
   * running and deletes the proxy and thumbnails, rather than holding them until the next open.
   */
  const close = useCallback(() => {
    openCount.current++;
    if (state.stage !== 'empty')
      // A failure leaves nothing for the user to do: the next open or exit releases the source.
      closeSource(state.path).catch(() => {});
    setState({ stage: 'empty' });
  }, [state]);

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
        failure: {
          text: `The preview copy of ${fileName(state.path)} could not play.`,
        },
      });
  }, [state, buildProxy]);

  return { state, dragging, chooseFile, openPath, close, unplayable };
}
