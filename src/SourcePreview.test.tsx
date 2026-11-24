import { act, fireEvent, render, screen } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';

import SourcePreview from './SourcePreview';
import type { SourceInfo } from './probe';

// Handlers the component registers with Tauri, so tests can play the backend's part.
const tauri = vi.hoisted(() => ({
  drop: null as null | ((event: { payload: unknown }) => void),
  progress: null as null | ((event: { payload: unknown }) => void),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  convertFileSrc: (path: string) => `asset://${path}`,
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (_name: string, handler: typeof tauri.progress) => {
    tauri.progress = handler;
    return () => {};
  }),
}));
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: async (handler: typeof tauri.drop) => {
      tauri.drop = handler;
      return () => {};
    },
  }),
}));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));

const info: SourceInfo = {
  duration: 12,
  width: 1920,
  height: 1080,
  frameRate: 30,
  videoCodec: 'h264',
  container: 'mov,mp4,m4a,3gp,3g2,mj2',
  hasAudio: true,
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

type Args = Record<string, unknown>;

/** Routes invoke() by command name to the given handlers. */
function backend(handlers: Record<string, (args: Args) => Promise<unknown>>) {
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    const handler = handlers[command];
    if (!handler) throw new Error(`unexpected command ${command}`);
    return handler(args as Args);
  });
}

async function drop(path: string) {
  await act(async () => {
    tauri.drop!({ payload: { type: 'drop', paths: [path], position: { x: 0, y: 0 } } });
  });
}

async function progress(source: string, value: number) {
  await act(async () => {
    tauri.progress!({ payload: { source, progress: value } });
  });
}

async function renderPreview() {
  render(<SourcePreview />);
  // Wait for the drop and progress listeners to be registered.
  await act(async () => {});
}

afterEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(open).mockReset();
  tauri.drop = null;
  tauri.progress = null;
});

test('opens a file chosen in the dialog and plays the source itself', async () => {
  const probe = deferred<SourceInfo>();
  backend({
    probe_source: () => probe.promise,
    open_source: async () => 'source',
  });
  vi.mocked(open).mockResolvedValue('C:\\videos\\clip.mp4');
  await renderPreview();

  fireEvent.click(screen.getByRole('button', { name: 'Open video…' }));
  expect(await screen.findByText('Opening clip.mp4…')).toBeInTheDocument();

  await act(async () => probe.resolve(info));
  const video = document.querySelector('video');
  expect(video).toHaveAttribute('src', 'asset://C:\\videos\\clip.mp4');
  expect(invoke).toHaveBeenCalledWith('probe_source', { path: 'C:\\videos\\clip.mp4' });
  expect(invoke).toHaveBeenCalledWith('open_source', {
    path: 'C:\\videos\\clip.mp4',
    info,
  });
  expect(invoke).not.toHaveBeenCalledWith('make_proxy', expect.anything());
});

test('shows why a dropped file cannot be opened, in plain words', async () => {
  backend({
    probe_source: async () => {
      throw {
        kind: 'unreadable',
        path: 'C:\\videos\\notes.mp4',
        reason: 'Invalid data found when processing input',
      };
    },
  });
  await renderPreview();

  await drop('C:\\videos\\notes.mp4');

  expect(await screen.findByRole('alert')).toHaveTextContent(
    'ffmpeg cannot read notes.mp4: Invalid data found when processing input'
  );
  expect(document.querySelector('video')).toBeNull();
});

test('builds a proxy with progress when WebView2 cannot play the source', async () => {
  const proxy = deferred<string>();
  backend({
    probe_source: async () => ({ ...info, videoCodec: 'hevc' }),
    open_source: async () => 'proxy',
    make_proxy: () => proxy.promise,
  });
  await renderPreview();

  await drop('C:\\videos\\phone.mkv');
  expect(
    await screen.findByText('Making a preview copy of phone.mkv: 0%')
  ).toBeInTheDocument();

  await progress('C:\\videos\\phone.mkv', 0.42);
  expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '42');
  // Progress for a source no longer open is ignored.
  await progress('C:\\videos\\other.mkv', 0.9);
  expect(screen.getByText('Making a preview copy of phone.mkv: 42%')).toBeInTheDocument();

  await act(async () => proxy.resolve('C:\\Temp\\video-to-gif\\session-1\\proxy-1.mp4'));
  expect(document.querySelector('video')).toHaveAttribute(
    'src',
    'asset://C:\\Temp\\video-to-gif\\session-1\\proxy-1.mp4'
  );
  expect(screen.getByText('phone.mkv (preview copy)')).toBeInTheDocument();
  expect(invoke).toHaveBeenCalledWith('make_proxy', { path: 'C:\\videos\\phone.mkv' });
});

test('falls back to a proxy when the player cannot decode the source', async () => {
  backend({
    probe_source: async () => info,
    open_source: async () => 'source',
    make_proxy: () => new Promise(() => {}),
  });
  await renderPreview();
  await drop('C:\\videos\\high10.mp4');

  const video = document.querySelector('video')!;
  await act(async () => {
    fireEvent.error(video);
  });

  expect(invoke).toHaveBeenCalledWith('make_proxy', { path: 'C:\\videos\\high10.mp4' });
  expect(screen.getByText(/Making a preview copy of high10\.mp4/)).toBeInTheDocument();
});

test('opening another source while a proxy builds shows only the new one', async () => {
  const firstProxy = deferred<string>();
  backend({
    probe_source: async ({ path }) =>
      path === 'C:\\videos\\first.mkv' ? { ...info, videoCodec: 'hevc' } : info,
    open_source: async ({ path }) =>
      path === 'C:\\videos\\first.mkv' ? 'proxy' : 'source',
    make_proxy: () => firstProxy.promise,
  });
  await renderPreview();
  await drop('C:\\videos\\first.mkv');
  expect(await screen.findByRole('progressbar')).toBeInTheDocument();

  await drop('C:\\videos\\second.mp4');
  // The backend cancels the first build when the second source opens.
  await act(async () => firstProxy.reject({ kind: 'cancelled' }));

  expect(screen.queryByRole('alert')).toBeNull();
  expect(screen.queryByRole('progressbar')).toBeNull();
  expect(document.querySelector('video')).toHaveAttribute(
    'src',
    'asset://C:\\videos\\second.mp4'
  );
});

test('says when ffmpeg fails to make the proxy', async () => {
  backend({
    probe_source: async () => ({ ...info, videoCodec: 'hevc' }),
    open_source: async () => 'proxy',
    make_proxy: async () => {
      throw { kind: 'proxyFailed', reason: 'Conversion failed!' };
    },
  });
  await renderPreview();
  await drop('C:\\videos\\broken.mkv');

  expect(await screen.findByRole('alert')).toHaveTextContent(
    'ffmpeg could not make a preview copy of broken.mkv: Conversion failed!'
  );
});
