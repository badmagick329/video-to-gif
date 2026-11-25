// Opening a source into the editor: probe, preview source or proxy, failures, and the source's
// displayed pixels standing in for a proxy's in every size the editor shows.
import { act, fireEvent, screen, within } from '@testing-library/react';

import Editor from '../Editor';
import { renderSourceEditor } from './harness';
import type { SourceInfo } from '../../ipc/probe';
import {
  backend,
  clipBackend,
  core,
  dialog,
  drop,
  mock2dCanvas,
  openClip,
  estimatedSize,
  exportButton,
  outputSize,
  progress,
  proxyPath,
  renderWithListeners,
  resetTauri,
  sourceInfo,
  tauri,
  toolsFound,
} from './tauri';

vi.mock('@tauri-apps/api/core', async () => (await import('./tauri')).core);
vi.mock('@tauri-apps/api/event', async () => (await import('./tauri')).event);
vi.mock('@tauri-apps/api/webview', async () => (await import('./tauri')).webview);
vi.mock('@tauri-apps/plugin-dialog', async () => (await import('./tauri')).dialog);

const info = sourceInfo({ duration: 12, width: 1920, height: 1080 });

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/** ffmpeg's own message is in the alert, folded until the user opens it. */
function expectFolded(alert: HTMLElement, reason: string) {
  const details = alert.querySelector('details')!;
  expect(details).not.toHaveAttribute('open');
  expect(within(details).getByText("ffmpeg's reason")).toBeInTheDocument();
  expect(within(details).getByText(reason)).toBeInTheDocument();
}

beforeEach(() => {
  vi.spyOn(window, 'requestAnimationFrame').mockImplementation(() => 0);
  mock2dCanvas();
});
afterEach(() => {
  resetTauri();
  vi.restoreAllMocks();
});

test('opens a file chosen with Open video and plays the source itself', async () => {
  const probe = deferred<SourceInfo>();
  backend({
    probe_source: () => probe.promise,
    open_source: async () => 'source',
    make_thumbnails: () => new Promise(() => {}),
  });
  dialog.open.mockResolvedValue('C:\\videos\\clip.mp4');
  await renderWithListeners(<Editor tools={toolsFound} />);

  fireEvent.click(screen.getByRole('button', { name: /^Open video/ }));
  expect(await screen.findByText('Opening clip.mp4…')).toBeInTheDocument();

  await act(async () => probe.resolve(info));
  expect(document.querySelector('video')).toHaveAttribute(
    'src',
    'asset://C:\\videos\\clip.mp4'
  );
  expect(screen.getByRole('button', { name: 'Close clip.mp4' })).toBeInTheDocument();
  expect(core.invoke).toHaveBeenCalledWith('probe_source', { path: 'C:\\videos\\clip.mp4' });
  expect(core.invoke).toHaveBeenCalledWith('open_source', {
    path: 'C:\\videos\\clip.mp4',
    info,
  });
  expect(core.invoke).not.toHaveBeenCalledWith('make_proxy', expect.anything());
});

test('Ctrl+O opens the file dialog', async () => {
  dialog.open.mockResolvedValue(null);
  await renderWithListeners(<Editor tools={toolsFound} />);
  fireEvent.keyDown(document.body, { key: 'o', ctrlKey: true });
  expect(dialog.open).toHaveBeenCalledTimes(1);
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
  await renderWithListeners(<Editor tools={toolsFound} />);

  await drop('C:\\videos\\notes.mp4');

  const alert = await screen.findByRole('alert');
  expect(alert).toHaveTextContent(/^ffmpeg cannot read notes\.mp4\./);
  expectFolded(alert, 'Invalid data found when processing input');
  expect(document.querySelector('video')).toBeNull();
});

test('builds a proxy with progress when WebView2 cannot play the source', async () => {
  const proxy = deferred<string>();
  backend({
    probe_source: async () => ({ ...info, videoCodec: 'hevc' }),
    open_source: async () => 'proxy',
    make_proxy: () => proxy.promise,
    make_thumbnails: () => new Promise(() => {}),
  });
  await renderWithListeners(<Editor tools={toolsFound} />);

  await drop('C:\\videos\\phone.mkv');
  expect(
    await screen.findByText('Making a preview copy of phone.mkv: 0%')
  ).toBeInTheDocument();
  // The settings work from the probe while the proxy builds.
  expect(outputSize()).toHaveTextContent('480 × 270');

  await progress('C:\\videos\\phone.mkv', 0.42);
  expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '42');
  // Progress for a source no longer open is ignored.
  await progress('C:\\videos\\other.mkv', 0.9);
  expect(screen.getByText('Making a preview copy of phone.mkv: 42%')).toBeInTheDocument();

  await act(async () => proxy.resolve(proxyPath));
  expect(document.querySelector('video')).toHaveAttribute('src', `asset://${proxyPath}`);
  expect(screen.getByText('Playing a preview copy')).toBeInTheDocument();
  expect(core.invoke).toHaveBeenCalledWith('make_proxy', { path: 'C:\\videos\\phone.mkv' });
});

test('falls back to a proxy when the player cannot decode the source, keeping the edit', async () => {
  backend({
    probe_source: async () => info,
    open_source: async () => 'source',
    make_proxy: () => new Promise(() => {}),
    make_thumbnails: () => new Promise(() => {}),
  });
  await renderWithListeners(<Editor tools={toolsFound} />);
  await drop('C:\\videos\\high10.mp4');
  const end = screen.getByRole('textbox', { name: 'Out' });
  fireEvent.change(end, { target: { value: '0:05' } });
  fireEvent.blur(end);

  // A video track it can't decode beside audio it can loads with no picture.
  await act(async () => {
    fireEvent.loadedMetadata(document.querySelector('video')!);
  });

  expect(core.invoke).toHaveBeenCalledWith('make_proxy', { path: 'C:\\videos\\high10.mp4' });
  expect(screen.getByText(/Making a preview copy of high10\.mp4/)).toBeInTheDocument();
  expect(estimatedSize()).toHaveTextContent('75 frames · 5.0 s');
});

test('opening another source while a proxy builds shows only the new one', async () => {
  const firstProxy = deferred<string>();
  backend({
    probe_source: async ({ path }) =>
      path === 'C:\\videos\\first.mkv' ? { ...info, videoCodec: 'hevc' } : info,
    open_source: async ({ path }) =>
      path === 'C:\\videos\\first.mkv' ? 'proxy' : 'source',
    make_proxy: () => firstProxy.promise,
    make_thumbnails: () => new Promise(() => {}),
  });
  await renderWithListeners(<Editor tools={toolsFound} />);
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
  await renderWithListeners(<Editor tools={toolsFound} />);
  await drop('C:\\videos\\broken.mkv');

  const alert = await screen.findByRole('alert');
  expect(alert).toHaveTextContent(
    /^ffmpeg could not make a preview copy of broken\.mkv\./
  );
  expectFolded(alert, 'Conversion failed!');
});

test('removing the clip mid-proxy closes its source in the backend', async () => {
  const proxy = deferred<string>();
  const closed: unknown[] = [];
  backend({
    probe_source: async () => ({ ...info, videoCodec: 'hevc' }),
    open_source: async () => 'proxy',
    make_proxy: () => proxy.promise,
    make_thumbnails: () => new Promise(() => {}),
    close_source: async (args) => {
      closed.push(args);
      // The backend stops the build, so the pending make_proxy fails as cancelled.
      proxy.reject({ kind: 'cancelled' });
    },
  });
  await renderWithListeners(<Editor tools={toolsFound} />);
  await drop('C:\\videos\\phone.mkv');
  expect(await screen.findByRole('progressbar')).toBeInTheDocument();

  await act(async () => {
    fireEvent.click(screen.getByRole('button', { name: 'Close phone.mkv' }));
  });

  expect(closed).toEqual([{ path: 'C:\\videos\\phone.mkv' }]);
  expect(screen.queryByRole('progressbar')).toBeNull();
  expect(screen.queryByRole('alert')).toBeNull();
  expect(document.querySelector('video')).toBeNull();
  expect(
    screen.getByRole('button', { name: /^Open video/ })
  ).toBeInTheDocument();
});

test('removing a clip that plays the source also closes it', async () => {
  clipBackend(info);
  const closeSource = vi.fn(async () => {});
  tauri.commands.close_source = closeSource;
  await renderWithListeners(<Editor tools={toolsFound} />);
  await drop('C:\\videos\\clip.mp4');

  await act(async () => {
    fireEvent.click(screen.getByRole('button', { name: 'Close clip.mp4' }));
  });

  expect(closeSource).toHaveBeenCalledWith({ path: 'C:\\videos\\clip.mp4' });
  expect(document.querySelector('video')).toBeNull();
});

describe('on a proxy, sizes are in the source’s displayed pixels', () => {
  // A 4K source shown through its 1280 × 720 proxy.
  const uhd = sourceInfo({ width: 3840, height: 2160 });
  const press = (name: string) =>
    fireEvent.click(screen.getByRole('button', { name }));

  async function openProxied() {
    clipBackend(uhd, 'proxy');
    await renderWithListeners(<Editor tools={toolsFound} />);
    return openClip('C:\\videos\\uhd.mkv', { width: 1280, height: 720 });
  }

  test('the crop readout and output size scale to the source, not the proxy', async () => {
    await openProxied();
    expect(document.querySelector('video')).toHaveAttribute('src', `asset://${proxyPath}`);
    expect(screen.getByText('3840 × 2160', { selector: 'span' })).toBeInTheDocument();

    press('1:1');
    expect(screen.getByText('Source · crop 2160 × 2160')).toBeInTheDocument();
    press('Original');
    expect(outputSize()).toHaveTextContent('2160 × 2160');
    // A preset narrower than the source's crop stays available, though the proxy is 720 wide.
    expect(screen.getByRole('button', { name: '640' })).toBeEnabled();
    expect(screen.getByRole('spinbutton', { name: 'Width in pixels' })).toHaveValue(2160);
  });

  test('a crop covering the proxy is full frame on the source too', async () => {
    await openProxied();
    press('16:9');
    expect(screen.getByText('Source · full frame, drag a corner to crop')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Reset crop' })).toBeDisabled();
  });

  test('the export request carries the crop as fractions of the source, at its pixel size', async () => {
    const { request } = renderSourceEditor({
      info: uhd,
      preview: 'proxy',
      frame: { width: 1280, height: 720 },
    });

    press('9:16');
    press('Original');
    expect(screen.getByText('Source · crop 1215 × 2160')).toBeInTheDocument();
    await act(async () =>
      fireEvent.click(exportButton())
    );
    expect(Math.round(request().crop!.width * uhd.width)).toBe(1215);
    expect(Math.round(request().crop!.height * uhd.height)).toBe(2160);
    // Export sizes the output from the source's info, not the proxy's 405 px.
    expect(request().info).toEqual(uhd);
  });
});
