// The size estimate as the editor schedules and shows it: nothing reaches Rust until the
// settings have been still for 1.5 s, a change kills the running estimate and reruns only the
// file types it affects, playback and exports pause it, and each ticked file type shows its
// figure, a placeholder while estimating, or the old figure greyed while updating.
import { act, fireEvent, screen } from '@testing-library/react';

import { renderSourceEditor } from './harness';
import {
  core,
  estimatedSize,
  estimateAnswers,
  invoked,
  mock2dCanvas,
  resetTauri,
  sourceInfo,
  tauri,
} from './tauri';

vi.mock('@tauri-apps/api/core', async () => (await import('./tauri')).core);
vi.mock('@tauri-apps/api/event', async () => (await import('./tauri')).event);
vi.mock('@tauri-apps/api/webview', async () => (await import('./tauri')).webview);
vi.mock('@tauri-apps/plugin-dialog', async () => (await import('./tauri')).dialog);

const MiB = 1024 * 1024;

beforeEach(() => {
  resetTauri();
  vi.useFakeTimers();
  mock2dCanvas();
});

afterEach(() => {
  vi.useRealTimers();
});

/** Lets `ms` of fake time pass and the promises it starts settle. */
async function wait(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

type EstimateArgs = {
  request: {
    settings: { width: number; fileTypes: string[]; gifEncoder: string };
  };
};

const estimateCalls = () =>
  invoked('estimate_output_size').map(
    ([, args]) => (args as EstimateArgs).request
  );

/** Opens a 1080p clip with GIF, WebP and MP4 ticked, and lets the first estimate run. */
async function openWithAllTicked({ gifskiFound = false } = {}) {
  const view = renderSourceEditor({
    info: sourceInfo({ width: 1920, height: 1080, duration: 60 }),
    gifskiFound,
  });
  tauri.commands.estimate_output_size = estimateAnswers({
    gif: 6 * MiB,
    webp: 2 * MiB,
    mp4: MiB,
  });
  fireEvent.click(screen.getByRole('button', { name: 'WebP' }));
  fireEvent.click(screen.getByRole('button', { name: 'MP4' }));
  await wait(1500);
  return view;
}

const width = () => screen.getByRole('slider', { name: 'Width' });

test('dragging the width slider starts no estimate until 1.5 s after the drag ends', async () => {
  await openWithAllTicked();
  expect(estimateCalls()).toHaveLength(1);

  // Three seconds of drag steps, one every 100 ms, each a new width.
  for (let step = 0; step < 30; step += 1) {
    fireEvent.change(width(), { target: { value: String(200 + step * 2) } });
    await wait(100);
  }
  expect(estimateCalls()).toHaveLength(1);

  // The last step was 100 ms ago.
  await wait(1399);
  expect(estimateCalls()).toHaveLength(1);
  await wait(1);
  expect(estimateCalls()).toHaveLength(2);
  expect(estimateCalls()[1]!.settings).toMatchObject({
    width: 258,
    fileTypes: ['gif', 'webp', 'mp4'],
  });
}, 20_000);

test('a change kills the running estimate at once', async () => {
  renderSourceEditor({ info: sourceInfo({ duration: 60 }) });
  // An estimate that never finishes.
  tauri.commands.estimate_output_size = () => new Promise(() => {});
  await wait(1500);
  expect(estimateCalls()).toHaveLength(1);
  expect(invoked('cancel_size_estimate')).toHaveLength(0);

  fireEvent.change(width(), { target: { value: '320' } });
  expect(invoked('cancel_size_estimate')).toHaveLength(1);
  // The next one still waits for the settings to settle.
  await wait(1499);
  expect(estimateCalls()).toHaveLength(1);
  await wait(1);
  expect(estimateCalls()).toHaveLength(2);
});

test('a change reruns only the file types it affects', async () => {
  await openWithAllTicked();
  expect(estimateCalls()[0]!.settings.fileTypes).toEqual(['gif', 'webp', 'mp4']);

  fireEvent.click(screen.getByRole('button', { name: 'GIF quality & looping' }));
  fireEvent.change(screen.getByRole('slider', { name: 'Colours' }), {
    target: { value: '64' },
  });
  await wait(1500);
  expect(estimateCalls()[1]!.settings.fileTypes).toEqual(['gif']);

  fireEvent.change(screen.getByRole('slider', { name: 'WebP quality' }), {
    target: { value: '60' },
  });
  await wait(1500);
  expect(estimateCalls()[2]!.settings.fileTypes).toEqual(['webp']);

  fireEvent.click(screen.getByRole('switch', { name: 'Keep sound in MP4' }));
  await wait(1500);
  expect(estimateCalls()[3]!.settings.fileTypes).toEqual(['mp4']);

  fireEvent.change(width(), { target: { value: '320' } });
  await wait(1500);
  expect(estimateCalls()[4]!.settings.fileTypes).toEqual(['gif', 'webp', 'mp4']);
  expect(estimateCalls()).toHaveLength(5);
});

test('the GIF encoder and gifski’s qualities rerun only the GIF', async () => {
  await openWithAllTicked({ gifskiFound: true });
  fireEvent.change(screen.getByRole('combobox', { name: 'GIF encoder' }), {
    target: { value: 'gifski' },
  });
  await wait(1500);
  expect(estimateCalls()[1]!.settings).toMatchObject({
    fileTypes: ['gif'],
    gifEncoder: 'gifski',
  });

  fireEvent.click(screen.getByRole('button', { name: 'GIF quality & looping' }));
  fireEvent.change(screen.getByRole('slider', { name: 'Motion quality' }), {
    target: { value: '50' },
  });
  await wait(1500);
  expect(estimateCalls()[2]!.settings.fileTypes).toEqual(['gif']);
  expect(estimateCalls()).toHaveLength(3);
});

test('estimates wait while the video plays', async () => {
  const { video } = renderSourceEditor({ info: sourceInfo({ duration: 60 }) });
  Object.defineProperty(video, 'paused', { value: false, configurable: true });
  fireEvent.play(video);
  await wait(3000);
  expect(estimateCalls()).toHaveLength(0);
  expect(screen.getByText('Sizes update when the video stops.')).toBeInTheDocument();

  Object.defineProperty(video, 'paused', { value: true, configurable: true });
  fireEvent.pause(video);
  await wait(1500);
  expect(estimateCalls()).toHaveLength(1);
});

test('each ticked file type shows its estimate, and the old one greyed while it updates', async () => {
  await openWithAllTicked();
  const details = estimatedSize();
  expect(details).toHaveTextContent('GIF ≈ 6.0 MiB');
  expect(details).toHaveTextContent('WebP ≈ 2.0 MiB');
  expect(details).toHaveTextContent('MP4 ≈ 1.0 MiB');
  expect(
    screen.getByText('Sizes come from samples of this selection, so expect some drift.')
  ).toBeInTheDocument();

  // A change greys the figures it affects until the new ones land.
  tauri.commands.estimate_output_size = estimateAnswers({
    gif: 3 * MiB,
    webp: MiB,
    mp4: MiB / 2,
  });
  fireEvent.change(width(), { target: { value: '320' } });
  expect(screen.getByLabelText('GIF about 6.0 MiB, updating')).toBeInTheDocument();
  expect(screen.getByText('Updating sizes…')).toBeInTheDocument();
  await wait(1500);
  expect(details).toHaveTextContent('GIF ≈ 3.0 MiB');
  expect(details).toHaveTextContent('MP4 ≈ 512.0 KiB');

  // Unticking and ticking again keeps a figure whose settings have not changed, and asks Rust
  // for nothing.
  const calls = estimateCalls().length;
  fireEvent.click(screen.getByRole('button', { name: 'GIF' }));
  fireEvent.click(screen.getByRole('button', { name: 'GIF' }));
  expect(details).toHaveTextContent('GIF ≈ 3.0 MiB');
  await wait(1500);
  expect(estimateCalls()).toHaveLength(calls);
});

test('a file type with no figure yet shows a placeholder', async () => {
  renderSourceEditor({ info: sourceInfo({ duration: 60 }) });
  expect(estimatedSize()).toContainElement(
    screen.getByRole('img', { name: 'Estimating' })
  );
  expect(
    screen.getByText('Encoding short samples of the selection…')
  ).toBeInTheDocument();
  expect(core.invoke).not.toHaveBeenCalledWith(
    'estimate_output_size',
    expect.anything()
  );
});
