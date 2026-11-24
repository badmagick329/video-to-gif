// Export through Rust's export: progress, cancel, failures, Made this session with Open and Show
// in folder, the frame-memory warning and where outputs are saved. The backend's part is played by
// ./tauri, with run_export held open so each test can emit events mid-run.
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';

import Editor from '../Editor';
import SessionResults from '../SessionResults';
import { sourceFolder } from '../useDestination';
import type { ExportOutput } from '../../ipc/export';
import {
  backend,
  clipBackend,
  core,
  dialog,
  emit,
  estimateAnswers,
  mock2dCanvas,
  openClip,
  renderWithListeners,
  resetTauri,
  sourceInfo,
  tauri,
  exportButton,
  ffmpegFound,
} from './tauri';

vi.mock('@tauri-apps/api/core', async () => (await import('./tauri')).core);
vi.mock('@tauri-apps/api/event', async () => (await import('./tauri')).event);
vi.mock('@tauri-apps/api/webview', async () => (await import('./tauri')).webview);
vi.mock('@tauri-apps/plugin-dialog', async () => (await import('./tauri')).dialog);

const source = 'C:\\videos\\one.mp4';
const gib = 1024 ** 3;

beforeEach(() => {
  vi.spyOn(window, 'requestAnimationFrame').mockImplementation(() => 0);
  mock2dCanvas();
});
afterEach(() => {
  resetTauri();
  vi.restoreAllMocks();
});

const done = (fileType: 'gif' | 'webp' | 'mp4', folder = 'C:\\videos'): ExportOutput => ({
  status: 'done',
  fileType,
  path: `${folder}\\one.${fileType}`,
  size: 1.5 * 1024 * 1024,
  width: 480,
  height: 270,
});

/** A run_export that stays open until the test settles it, recording what it was asked. */
function heldExport() {
  const calls: Array<Record<string, unknown>> = [];
  let settle!: (outputs: ExportOutput[]) => void;
  let refuse!: (error: unknown) => void;
  tauri.commands.run_export = (args) => {
    calls.push(args);
    return new Promise((resolve, reject) => {
      settle = resolve;
      refuse = reject;
    });
  };
  return {
    calls,
    finish: (outputs: ExportOutput[]) => act(async () => settle(outputs)),
    reject: (error: unknown) => act(async () => refuse(error)),
  };
}

/** Opens a 4-second 1280 × 720 clip from C:\videos with the given extra commands. */
async function openEditor(commands: Record<string, (args: Record<string, unknown>) => Promise<unknown>> = {}) {
  clipBackend(sourceInfo());
  Object.assign(tauri.commands, {
    open_output: async () => {},
    show_output_in_folder: async () => {},
    remember_destination: async () => {},
    cancel_export: async () => {},
    ...commands,
  });
  const view = await renderWithListeners(<Editor ffmpeg={ffmpegFound} />);
  await openClip(source, { width: 1280, height: 720 });
  return view;
}

/** Shows an inspector tab; the other one's content leaves the accessibility tree. */
const showTab = (name: 'Output' | 'Exports') =>
  fireEvent.click(screen.getByRole('tab', { name: new RegExp(`^${name}`) }));
const runExport = () => act(async () => fireEvent.click(exportButton()));
const progressBar = () => {
  showTab('Output');
  return screen.getByRole('progressbar', { name: 'Export progress' });
};
const results = () => {
  showTab('Exports');
  return screen.getByRole('region', { name: 'Made this session' });
};
const saveTo = () => {
  showTab('Output');
  return screen.getByRole('group', { name: 'Save to' });
};
const invoked = (command: string) =>
  core.invoke.mock.calls.filter(([name]) => name === command).map(([, args]) => args);

describe('Export', () => {
  test('runs the export into the video’s folder, shows each file type’s progress, and lists each output as it lands', async () => {
    await openEditor();
    fireEvent.click(screen.getByRole('button', { name: 'WebP' }));
    fireEvent.click(screen.getByRole('button', { name: 'MP4' }));
    const run = heldExport();
    await runExport();

    expect(run.calls).toHaveLength(1);
    expect(run.calls[0]).toMatchObject({
      destination: 'C:\\videos',
      request: { path: source, settings: { fileTypes: ['gif', 'webp', 'mp4'] } },
    });
    // Export gives way to the progress bar and Cancel; settings lock.
    expect(screen.queryByRole('button', { name: /^Export / })).toBeNull();
    expect(screen.getByRole('button', { name: 'WebP' })).toBeDisabled();

    await emit('export-progress', { fileType: 'gif', progress: 0 });
    expect(progressBar()).toHaveTextContent('Making GIF (1 of 3), analysing colours…');
    await emit('export-output', done('gif'));
    await emit('export-progress', { fileType: 'webp', progress: 0.5 });
    expect(progressBar()).toHaveTextContent('Making WebP (2 of 3) · 50%');
    expect(progressBar()).toHaveAttribute('aria-valuenow', '50');

    // The GIF is listed while WebP is still being made, playing from its file.
    const gifRow = within(results()).getByRole('listitem');
    expect(gifRow).toHaveTextContent('one.gif');
    expect(gifRow).toHaveTextContent('GIF · 480 × 270 · 1.5 MiB');
    expect(within(gifRow).getByRole('img', { name: 'GIF preview' })).toHaveAttribute(
      'src',
      'asset://C:\\videos\\one.gif'
    );

    await emit('export-output', done('webp'));
    await emit('export-output', done('mp4'));
    await run.finish([done('gif'), done('webp'), done('mp4')]);

    showTab('Output');
    expect(screen.getByRole('status')).toHaveTextContent(
      'Saved GIF, WebP and MP4 to C:\\videos.'
    );
    // Events and the resolved list both carry each output; it is listed once.
    const rows = within(results()).getAllByRole('listitem');
    expect(rows.map((row) => within(row).getByText(/^one\./).textContent)).toEqual([
      'one.gif',
      'one.webp',
      'one.mp4',
    ]);
    expect(within(rows[2]!).getByLabelText('MP4 preview').tagName).toBe('VIDEO');
    showTab('Output');
    expect(exportButton()).toBeEnabled();
  });

  test('lists an output whose event never came, from what run_export resolved with', async () => {
    await openEditor();
    const run = heldExport();
    await runExport();
    await run.finish([done('gif')]);
    expect(within(results()).getByRole('listitem')).toHaveTextContent('one.gif');
  });

  test('Cancel stops the run; the file types it stopped leave nothing in the list', async () => {
    await openEditor();
    fireEvent.click(screen.getByRole('button', { name: 'WebP' }));
    const run = heldExport();
    await runExport();
    await emit('export-output', done('gif'));
    await emit('export-progress', { fileType: 'webp', progress: 0.3 });

    showTab('Output');
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(invoked('cancel_export')).toHaveLength(1);
    expect(screen.getByRole('button', { name: 'Cancelling…' })).toBeDisabled();

    await emit('export-output', { status: 'cancelled', fileType: 'webp' });
    await run.finish([done('gif'), { status: 'cancelled', fileType: 'webp' }]);
    expect(screen.getByRole('status')).toHaveTextContent(
      'Cancelled; nothing unfinished was left behind. GIF saved to C:\\videos.'
    );
    expect(screen.queryByRole('progressbar', { name: 'Export progress' })).toBeNull();
    expect(within(results()).getAllByRole('listitem')).toHaveLength(1);
    expect(screen.getByRole('tab', { name: /^Exports/ })).toHaveTextContent('Exports1');
  });

  test('a failed file type says so briefly and folds ffmpeg’s error away', async () => {
    await openEditor();
    fireEvent.click(screen.getByRole('button', { name: 'WebP' }));
    const run = heldExport();
    await runExport();
    const tail = '[libwebp_anim @ 0x1] Encoding error\nConversion failed!';
    await run.finish([done('gif'), { status: 'failed', fileType: 'webp', reason: tail }]);

    expect(screen.getByRole('alert')).toHaveTextContent(
      "WebP failed; ffmpeg's error is in the Exports tab. GIF saved to C:\\videos."
    );
    const failed = within(results()).getAllByRole('listitem')[1]!;
    expect(failed).toHaveTextContent('WebP conversion failed');
    const details = failed.querySelector('details')!;
    expect(details.open).toBe(false);
    fireEvent.click(within(failed).getByText("ffmpeg's error"));
    expect(details.open).toBe(true);
    expect(failed.querySelector('pre')!.textContent).toBe(tail);
  });

  test('a run Rust refuses to start shows why, with its reason folded away', async () => {
    await openEditor();
    const run = heldExport();
    await runExport();
    await run.reject({ kind: 'invalidRequest', reason: 'the crop lies outside the frame' });
    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent('These settings cannot be exported.');
    expect(alert.querySelector('pre')).toHaveTextContent('the crop lies outside the frame');
    expect(exportButton()).toBeEnabled();
    expect(within(results()).queryByRole('listitem')).toBeNull();
  });

  test('a destination deleted since it was picked is reported, not written to', async () => {
    await openEditor();
    const run = heldExport();
    await runExport();
    await run.reject({ kind: 'destinationNotFound', path: 'C:\\videos' });
    expect(screen.getByRole('alert')).toHaveTextContent(
      'There is no folder at C:\\videos any more. Pick another with Save to.'
    );
  });
});

describe('Running card', () => {
  const MiB = 1024 * 1024;
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());
  const wait = (ms: number) => act(async () => void (await vi.advanceTimersByTimeAsync(ms)));
  const card = () => {
    showTab('Exports');
    return screen.getByRole('region', { name: /^Running/ });
  };
  const rows = () => within(card()).getAllByRole('listitem').map((row) => row.textContent);

  test('shows each ticked file type: done with its size, the one being made with percent, time left and estimate, the rest queued', async () => {
    await openEditor({
      estimate_output_size: estimateAnswers({ gif: 61 * MiB, webp: 8.4 * MiB, mp4: 5.3 * MiB }),
    });
    fireEvent.click(screen.getByRole('button', { name: 'WebP' }));
    fireEvent.click(screen.getByRole('button', { name: 'MP4' }));
    // Estimates start once the settings have been still for 1.5 s.
    await wait(1600);
    const run = heldExport();
    await runExport();

    expect(rows()).toEqual(['GIF0%est 61.0 MiB', 'WebPQueued', 'MP4Queued']);
    await emit('export-output', done('gif'));
    await wait(4000);
    await emit('export-progress', { fileType: 'webp', progress: 0.4 });

    expect(rows()).toEqual(['GIF1.5 MiB✓', 'WebP40% · 6.0 s leftest 8.4 MiB', 'MP4Queued']);
    expect(within(card()).getByRole('listitem', { name: 'GIF done, 1.5 MiB' })).toBeInTheDocument();
    expect(within(card()).getByRole('progressbar', { name: 'WebP progress' })).toHaveAttribute(
      'aria-valuenow',
      '40'
    );
    expect(card()).toHaveTextContent('started 0:04 ago');

    // The overall bar and Cancel are pinned under this tab too: GIF done and WebP 40% of three.
    const bar = screen.getByRole('progressbar', { name: 'Export progress' });
    expect(bar).toHaveTextContent('Making WebP (2 of 3) · 40%');
    expect(bar).toHaveAttribute('aria-valuenow', '47');
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(invoked('cancel_export')).toHaveLength(1);
    expect(screen.getByRole('button', { name: 'Cancelling…' })).toBeDisabled();

    await emit('export-output', done('webp'));
    await emit('export-output', done('mp4'));
    await run.finish([done('gif'), done('webp'), done('mp4')]);
    expect(screen.queryByRole('region', { name: /^Running/ })).toBeNull();
  });
});

describe('Made this session', () => {
  test('Open and Show in folder send Rust the output’s own path', async () => {
    await openEditor();
    const run = heldExport();
    await runExport();
    await run.finish([done('gif')]);

    showTab('Exports');
    fireEvent.click(screen.getByRole('button', { name: 'Open one.gif' }));
    fireEvent.click(screen.getByRole('button', { name: 'Show one.gif in folder' }));
    expect(invoked('open_output')).toEqual([{ path: 'C:\\videos\\one.gif' }]);
    expect(invoked('show_output_in_folder')).toEqual([{ path: 'C:\\videos\\one.gif' }]);
  });

  test('an output moved away since it was made says so when opened', async () => {
    render(
      <SessionResults
        results={[
          {
            id: 'a',
            fileType: 'gif',
            path: 'C:\\videos\\one.gif',
            name: 'one.gif',
            url: 'asset://one.gif',
            size: 1000,
            width: 480,
            height: 270,
          },
        ]}
      />
    );
    backend({
      open_output: async () => {
        throw { kind: 'missing', path: 'C:\\videos\\one.gif' };
      },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Open one.gif' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'one.gif has been moved or deleted.'
    );
  });
});

describe('Frame-memory guard', () => {
  test('warns that Windows will page, and Export anyway sends the acceptance', async () => {
    await openEditor({
      estimate_export_memory: async () => ({
        heldBytes: 6 * gib,
        availableBytes: 4 * gib,
        limitSeconds: 2.5,
        fits: false,
      }),
      run_export: () => new Promise(() => {}),
    });
    const anyway = await screen.findByRole('button', { name: 'Export anyway' });
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Making a GIF holds about 6.0 GiB of frames in memory and 4.0 GiB is free, so Windows will page to disk and the PC may crawl. Lower the width or frame rate, or trim to 0:02.50 or less, to stay in memory.'
    );
    expect(anyway).toBeEnabled();
    fireEvent.click(anyway);
    await waitFor(() => expect(invoked('run_export')).toHaveLength(1));
    expect(invoked('run_export')[0]).toMatchObject({
      request: { acceptPaging: true },
    });
    // The estimate is asked for the clip and settings Export would send.
    expect(invoked('estimate_export_memory').at(-1)).toMatchObject({
      request: { info: sourceInfo(), settings: { fileTypes: ['gif'] } },
    });
  });

  test('warns, without blocking, when the frames would take most of the free memory', async () => {
    await openEditor({
      estimate_export_memory: async () => ({
        heldBytes: 3 * gib,
        availableBytes: 4 * gib,
        limitSeconds: 5.5,
        fits: true,
      }),
    });
    expect(
      await screen.findByText(
        'Making a GIF will keep about 3.0 GiB of frames in memory, more than half of the 4.0 GiB free now.'
      )
    ).toBeInTheDocument();
    expect(exportButton()).toBeEnabled();
  });

  test('says nothing when the frames stream through', async () => {
    await openEditor();
    await waitFor(() => expect(invoked('estimate_export_memory')).not.toHaveLength(0));
    await act(async () => {});
    expect(screen.queryByText(/in memory/)).toBeNull();
  });
});

describe('Save to', () => {
  // Rust's saved_destination answers null with nothing remembered, and also when the remembered
  // folder no longer exists (destination.rs tests that), so this is the fallback too.
  test('defaults to the video’s folder', async () => {
    await openEditor();
    expect(saveTo()).toHaveTextContent('Save to C:\\videos Change…');
    expect(within(saveTo()).getByText('C:\\videos')).toHaveAttribute(
      'title',
      "C:\\videos (the video's folder)"
    );
    expect(within(saveTo()).queryByRole('button', { name: "Use the video's folder" })).toBeNull();
  });

  test('a folder remembered from an earlier launch is used and shown', async () => {
    await openEditor({ saved_destination: async () => 'D:\\gifs' });
    await waitFor(() => expect(saveTo()).toHaveTextContent('Save to D:\\gifs'));
    expect(within(saveTo()).getByText('D:\\gifs')).toHaveAttribute('title', 'D:\\gifs');
    const run = heldExport();
    await runExport();
    expect(run.calls[0]).toMatchObject({ destination: 'D:\\gifs' });
  });

  test('picking a folder overrides the default and is remembered; the video’s folder can be restored', async () => {
    await openEditor();
    dialog.open.mockResolvedValue('E:\\exports');
    await act(async () =>
      fireEvent.click(within(saveTo()).getByRole('button', { name: 'Change…' }))
    );
    expect(dialog.open).toHaveBeenCalledWith(
      expect.objectContaining({ directory: true, defaultPath: 'C:\\videos' })
    );
    expect(invoked('remember_destination')).toEqual([{ folder: 'E:\\exports' }]);
    expect(saveTo()).toHaveTextContent('Save to E:\\exports');

    const run = heldExport();
    await runExport();
    expect(run.calls[0]).toMatchObject({ destination: 'E:\\exports' });
    await run.finish([done('gif', 'E:\\exports')]);

    await act(async () =>
      fireEvent.click(within(saveTo()).getByRole('button', { name: "Use the video's folder" }))
    );
    expect(invoked('remember_destination').at(-1)).toEqual({ folder: null });
    expect(saveTo()).toHaveTextContent('Save to C:\\videos');
  });

  test('closing the folder picker keeps the current folder', async () => {
    await openEditor();
    dialog.open.mockResolvedValue(null);
    await act(async () =>
      fireEvent.click(within(saveTo()).getByRole('button', { name: 'Change…' }))
    );
    expect(invoked('remember_destination')).toEqual([]);
    expect(saveTo()).toHaveTextContent('Save to C:\\videos');
  });

  test('the video’s folder is the folder part of its path, a drive root included', () => {
    expect(sourceFolder('C:\\videos\\one.mp4')).toBe('C:\\videos');
    expect(sourceFolder('C:\\one.mp4')).toBe('C:\\');
    expect(sourceFolder('\\\\nas\\share\\one.mp4')).toBe('\\\\nas\\share');
    expect(sourceFolder('C:/videos/one.mp4')).toBe('C:/videos');
  });
});
