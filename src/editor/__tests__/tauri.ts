// A stand-in for the Tauri APIs the editor uses, so tests can play the backend's part. Each test
// file mocks the modules with these, e.g.
//   vi.mock('@tauri-apps/api/core', async () => (await import('./tauri')).core);
import { act, fireEvent, render, screen } from '@testing-library/react';
import type { ReactElement } from 'react';

import type { SourceInfo } from '../../ipc/probe';

type Args = Record<string, unknown>;
type Handler = (args: Args) => Promise<unknown>;

type Listener = (event: { payload: unknown }) => void;

export const tauri = {
  drop: null as null | Listener,
  /** The latest handler listening for each event name. */
  listeners: {} as Record<string, Listener>,
  commands: {} as Record<string, Handler>,
};

export const core = {
  invoke: vi.fn(async (command: string, args?: Args) => {
    const handler = tauri.commands[command];
    if (!handler) throw new Error(`unexpected command ${command}`);
    return handler(args ?? {});
  }),
  convertFileSrc: (path: string) => `asset://${path}`,
};

export const event = {
  listen: vi.fn(async (name: string, handler: Listener) => {
    tauri.listeners[name] = handler;
    return () => {
      if (tauri.listeners[name] === handler) delete tauri.listeners[name];
    };
  }),
};

/** Delivers an event to its listener, as Rust's emit would. */
export async function emit(name: string, payload: unknown) {
  const listener = tauri.listeners[name];
  if (!listener) throw new Error(`nothing listens for ${name}`);
  await act(async () => listener({ payload }));
}

export const webview = {
  getCurrentWebview: () => ({
    onDragDropEvent: async (handler: typeof tauri.drop) => {
      tauri.drop = handler;
      return () => {};
    },
  }),
};

export const dialog = { open: vi.fn() };

/** Routes invoke() by command name to the given handlers. */
export function backend(handlers: Record<string, Handler>) {
  tauri.commands = handlers;
}

export function resetTauri() {
  core.invoke.mockClear();
  dialog.open.mockReset();
  tauri.drop = null;
  tauri.listeners = {};
  tauri.commands = {};
}

/** Renders `ui` and waits for its drop and progress listeners to be registered. */
export async function renderWithListeners(ui: ReactElement) {
  const view = render(ui);
  await act(async () => {});
  return view;
}

export async function drop(path: string) {
  await act(async () => {
    tauri.drop!({
      payload: { type: 'drop', paths: [path], position: { x: 0, y: 0 } },
    });
  });
}

export async function progress(source: string, value: number) {
  await emit('proxy-progress', { source, progress: value });
}

export const sourceInfo = (patch: Partial<SourceInfo> = {}): SourceInfo => ({
  duration: 4,
  width: 1280,
  height: 720,
  frameRate: 30,
  videoCodec: 'h264',
  container: 'mov,mp4,m4a,3gp,3g2,mj2',
  audioCodec: 'aac',
  ...patch,
});

export const proxyPath = 'C:\\Temp\\video-to-gif\\session-1\\proxy-1.mp4';

/**
 * A backend with one openable source: played directly, or through a proxy of `proxySize`.
 * Thumbnails never arrive unless a test supplies them.
 */
export function clipBackend(
  info: SourceInfo,
  preview: 'source' | 'proxy' = 'source'
) {
  backend({
    probe_source: async () => info,
    open_source: async () => preview,
    make_proxy: async () => proxyPath,
    make_thumbnails: () => new Promise(() => {}),
    saved_destination: async () => null,
    estimate_export_memory: async () => memoryFits,
  });
}

/** An export that holds no frames, as forward WebP and MP4 do. */
export const memoryFits = {
  heldBytes: 0,
  availableBytes: 8 * 1024 ** 3,
  limitSeconds: null,
  fits: true,
};

/**
 * Drops `path` on the window and gives the player a decoded frame of `frame` pixels, which is
 * the proxy's size when the clip plays through one. jsdom never decodes video.
 */
export async function openClip(
  path: string,
  frame: { width: number; height: number }
) {
  await drop(path);
  const video = document.querySelector('video')!;
  Object.defineProperties(video, {
    videoWidth: { value: frame.width, configurable: true },
    videoHeight: { value: frame.height, configurable: true },
    readyState: { value: 4, configurable: true },
  });
  fireEvent.loadedMetadata(video);
  fireEvent.loadedData(video);
  return video;
}

/** The 2D context jsdom lacks, recording draws; WebGL is absent, so previews take the 2D route. */
export function mock2dCanvas(drawImage = vi.fn()) {
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockImplementation(((
    type: string
  ) =>
    type === '2d'
      ? { clearRect: vi.fn(), setTransform: vi.fn(), drawImage }
      : null) as unknown as HTMLCanvasElement['getContext']);
  return drawImage;
}

export const outputDetails = () => screen.getByLabelText('Output details');
