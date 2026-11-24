// Convert through Rust's export: progress, cancel, failures, Made this session with Open and Show
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
  mock2dCanvas,
  openClip,
  renderWithListeners,
  resetTauri,
  sourceInfo,
  tauri,
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
  const view = await renderWithListeners(<Editor />);
  await openClip(source, { width: 1280, height: 720 });
  return view;
}

const convertButton = () => screen.getByRole('button', { name: /^Convert to/ });
const convert = () => act(async () => fireEvent.click(convertButton()));
const progressBar = () => screen.getByRole('progressbar', { name: 'Conversion progress' });
const results = () => screen.getByRole('region', { name: 'Made this session' });
const saveTo = () => screen.getByRole('group', { name: 'Save to' });
const invoked = (command: string) =>
  core.invoke.mock.calls.filter(([name]) => name === command).map(([, args]) => args);

describe('Convert', () => {
  test('runs the export into the video’s folder, shows each file type’s progress, and lists each output as it lands', async () => {
    await openEditor();
    fireEvent.click(screen.getByRole('checkbox', { name: /WebP/ }));
    fireEvent.click(screen.getByRole('checkbox', { name: /MP4/ }));
    const run = heldExport();
    await convert();

    expect(run.calls).toHaveLength(1);
    expect(run.calls[0]).toMatchObject({
      destination: 'C:\\videos',
      request: { path: source, settings: { fileTypes: ['gif', 'webp', 'mp4'] } },
    });
    // Convert gives way to the progress bar and Cancel; settings lock.
    expect(screen.queryByRole('button', { name: /^Convert to/ })).toBeNull();
    expect(screen.getByRole('checkbox', { name: /WebP/ })).toBeDisabled();

    await emit('export-progress', { fileType: 'gif', progress: 0 });
    expect(progressBar()).toHaveTextContent('Making GIF (1/3), analysing colours…');
    await emit('export-output', done('gif'));
    await emit('export-progress', { fileType: 'webp', progress: 0.5 });
    expect(progressBar()).toHaveTextContent('Making WebP (2/3) 50%');
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
    expect(convertButton()).toBeEnabled();
  });

  test('lists an output whose event never came, from what run_export resolved with', async () => {
    await openEditor();
    const run = heldExport();
    await convert();
    await run.finish([done('gif')]);
    expect(within(results()).getByRole('listitem')).toHaveTextContent('one.gif');
  });

  test('Cancel stops the run; the file types it stopped leave nothing in the list', async () => {
    await openEditor();
    fireEvent.click(screen.getByRole('checkbox', { name: /WebP/ }));
    const run = heldExport();
    await convert();
    await emit('export-output', done('gif'));
    await emit('export-progress', { fileType: 'webp', progress: 0.3 });

    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(invoked('cancel_export')).toHaveLength(1);
    expect(screen.getByRole('button', { name: 'Cancelling…' })).toBeDisabled();

    await emit('export-output', { status: 'cancelled', fileType: 'webp' });
    await run.finish([done('gif'), { status: 'cancelled', fileType: 'webp' }]);
    expect(screen.getByRole('status')).toHaveTextContent(
      'Cancelled; nothing unfinished was left behind. GIF saved to C:\\videos.'
    );
    expect(within(results()).getAllByRole('listitem')).toHaveLength(1);
    expect(screen.queryByRole('progressbar', { name: 'Conversion progress' })).toBeNull();
  });

  test('a failed file type says so briefly and folds ffmpeg’s error away', async () => {
    await openEditor();
    fireEvent.click(screen.getByRole('checkbox', { name: /WebP/ }));
    const run = heldExport();
    await convert();
    const tail = '[libwebp_anim @ 0x1] Encoding error\nConversion failed!';
    await run.finish([done('gif'), { status: 'failed', fileType: 'webp', reason: tail }]);

    expect(screen.getByRole('alert')).toHaveTextContent(
      "WebP failed; ffmpeg's error is under Made this session. GIF saved to C:\\videos."
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
    await convert();
    await run.reject({ kind: 'invalidRequest', reason: 'the crop lies outside the frame' });
    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent('These settings cannot be converted.');
    expect(alert.querySelector('pre')).toHaveTextContent('the crop lies outside the frame');
    expect(within(results()).queryByRole('listitem')).toBeNull();
    expect(convertButton()).toBeEnabled();
  });

  test('a destination deleted since it was picked is reported, not written to', async () => {
    await openEditor();
    const run = heldExport();
    await convert();
    await run.reject({ kind: 'destinationNotFound', path: 'C:\\videos' });
    expect(screen.getByRole('alert')).toHaveTextContent(
      'There is no folder at C:\\videos any more. Pick another with Save to.'
    );
  });
});

describe('Made this session', () => {
  test('Open and Show in folder send Rust the output’s own path', async () => {
    await openEditor();
    const run = heldExport();
    await convert();
    await run.finish([done('gif')]);

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
  test('warns that Windows will page, and Convert anyway sends the acceptance', async () => {
    await openEditor({
      estimate_export_memory: async () => ({
        heldBytes: 6 * gib,
        availableBytes: 4 * gib,
        limitSeconds: 2.5,
        fits: false,
      }),
      run_export: () => new Promise(() => {}),
    });
    const anyway = await screen.findByRole('button', { name: 'Convert anyway' });
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Making a GIF holds about 6.0 GiB of frames in memory and 4.0 GiB is free, so Windows will page to disk and the PC may crawl. Lower the width or frame rate, or trim to 0:02.50 or less, to stay in memory.'
    );
    expect(anyway).toBeEnabled();
    fireEvent.click(anyway);
    await waitFor(() => expect(invoked('run_export')).toHaveLength(1));
    expect(invoked('run_export')[0]).toMatchObject({
      request: { acceptPaging: true },
    });
    // The estimate is asked for the clip and settings Convert would send.
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
    expect(convertButton()).toBeEnabled();
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
    expect(saveTo()).toHaveTextContent("Save to C:\\videos (the video's folder)");
    expect(within(saveTo()).queryByRole('button', { name: "Use the video's folder" })).toBeNull();
  });

  test('a folder remembered from an earlier launch is used and shown', async () => {
    await openEditor({ saved_destination: async () => 'D:\\gifs' });
    await waitFor(() => expect(saveTo()).toHaveTextContent('Save to D:\\gifs'));
    expect(saveTo()).not.toHaveTextContent("(the video's folder)");
    const run = heldExport();
    await convert();
    expect(run.calls[0]).toMatchObject({ destination: 'D:\\gifs' });
  });

  test('picking a folder overrides the default and is remembered; the video’s folder can be restored', async () => {
    await openEditor();
    dialog.open.mockResolvedValue('E:\\exports');
    await act(async () =>
      fireEvent.click(within(saveTo()).getByRole('button', { name: 'Choose folder…' }))
    );
    expect(dialog.open).toHaveBeenCalledWith(
      expect.objectContaining({ directory: true, defaultPath: 'C:\\videos' })
    );
    expect(invoked('remember_destination')).toEqual([{ folder: 'E:\\exports' }]);
    expect(saveTo()).toHaveTextContent('Save to E:\\exports');

    const run = heldExport();
    await convert();
    expect(run.calls[0]).toMatchObject({ destination: 'E:\\exports' });
    await run.finish([done('gif', 'E:\\exports')]);

    await act(async () =>
      fireEvent.click(within(saveTo()).getByRole('button', { name: "Use the video's folder" }))
    );
    expect(invoked('remember_destination').at(-1)).toEqual({ folder: null });
    expect(saveTo()).toHaveTextContent("Save to C:\\videos (the video's folder)");
  });

  test('closing the folder picker keeps the current folder', async () => {
    await openEditor();
    dialog.open.mockResolvedValue(null);
    await act(async () =>
      fireEvent.click(within(saveTo()).getByRole('button', { name: 'Choose folder…' }))
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
