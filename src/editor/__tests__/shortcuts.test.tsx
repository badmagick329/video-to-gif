// Copied from mgck's __tests__/discordgifs-shortcuts.test.tsx, for the one clip the editor
// holds; the tests of several simple-mode cards stay in mgck.
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';

import Editor from '../Editor';
import {
  clipBackend,
  core,
  dialog,
  drop,
  resetTauri,
  sourceInfo,
} from './tauri';

vi.mock('@tauri-apps/api/core', async () => (await import('./tauri')).core);
vi.mock('@tauri-apps/api/event', async () => (await import('./tauri')).event);
vi.mock('@tauri-apps/api/webview', async () => (await import('./tauri')).webview);
vi.mock('@tauri-apps/plugin-dialog', async () => (await import('./tauri')).dialog);

const frame = 1 / 30;
let played: HTMLMediaElement[];
let paused: HTMLMediaElement[];

beforeEach(() => {
  dialog.open.mockResolvedValue(null);
  played = [];
  paused = [];
  vi.spyOn(HTMLMediaElement.prototype, 'play').mockImplementation(function (
    this: HTMLMediaElement
  ) {
    played.push(this);
    return Promise.resolve();
  });
  vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(function (
    this: HTMLMediaElement
  ) {
    paused.push(this);
  });
  vi.spyOn(window, 'requestAnimationFrame').mockImplementation(() => 0);
});
afterEach(() => {
  resetTauri();
  vi.restoreAllMocks();
});

/** Opens a 60-second clip by dropping it and waits until it can be edited. */
async function loadClip(): Promise<[HTMLVideoElement]> {
  clipBackend(sourceInfo({ duration: 60 }));
  const { container } = render(<Editor />);
  await act(async () => {});
  await drop('C:/videos/clip.mp4');
  await waitFor(() =>
    expect(screen.getByRole('slider', { name: 'Playhead' })).toBeInTheDocument()
  );
  return [container.querySelector('video')!];
}
/** Whether Convert reached Rust's run_export. */
const exported = () => core.invoke.mock.calls.some(([command]) => command === 'run_export');

const press = (
  key: string,
  options: KeyboardEventInit = {},
  target: Element = document.body
) => fireEvent.keyDown(target, { key, ...options });
const startField = (index = 0) =>
  screen.getAllByLabelText('Start')[index] as HTMLInputElement;
const endField = (index = 0) =>
  screen.getAllByLabelText('End')[index] as HTMLInputElement;

test('each shortcut drives the clip editor', async () => {
  const [video] = await loadClip();

  press(' ');
  expect(played).toEqual([video]);
  fireEvent.play(video);
  press(' ');
  expect(paused).toContain(video);
  fireEvent.pause(video);

  video.currentTime = 10;
  press('ArrowRight');
  expect(video.currentTime).toBeCloseTo(10 + frame);
  press('ArrowLeft');
  press('ArrowLeft');
  expect(video.currentTime).toBeCloseTo(10 - frame);
  press('ArrowRight', { shiftKey: true });
  expect(video.currentTime).toBeCloseTo(11 - frame);
  press('ArrowLeft', { shiftKey: true });
  press('ArrowLeft', { shiftKey: true });
  expect(video.currentTime).toBeCloseTo(9 - frame);

  video.currentTime = 10;
  press('i');
  expect(startField().value).toBe('0:10.00');
  video.currentTime = 14;
  press('O');
  expect(endField().value).toBe('0:14.00');

  press('Home');
  expect(video.currentTime).toBe(10);
  press('End');
  expect(video.currentTime).toBe(14);

  press('Enter', { ctrlKey: true });
  await waitFor(() => expect(exported()).toBe(true));

  press('o', { ctrlKey: true });
  expect(dialog.open).toHaveBeenCalledTimes(1);
});

test('nothing fires while typing in a field, and focused sliders keep their keys', async () => {
  const [video] = await loadClip();
  video.currentTime = 10;
  const field = startField();
  act(() => field.focus());
  for (const [key, options] of [
    [' ', {}],
    ['i', {}],
    ['ArrowRight', {}],
    ['End', {}],
    ['Enter', { ctrlKey: true }],
    ['o', { ctrlKey: true }],
  ] as const)
    press(key, options, field);
  expect(played).toEqual([]);
  expect(video.currentTime).toBe(10);
  expect(field.value).toBe('0:00.00');
  expect(exported()).toBe(false);
  expect(dialog.open).not.toHaveBeenCalled();

  // Home on the playhead slider goes to the clip's start, not the selection's.
  fireEvent.change(field, { target: { value: '0:05' } });
  fireEvent.keyDown(field, { key: 'Enter' });
  const playhead = screen.getByRole('slider', { name: 'Playhead' });
  act(() => playhead.focus());
  press('Home', {}, playhead);
  expect(video.currentTime).toBe(0);
  press('ArrowRight', { shiftKey: true }, playhead);
  expect(video.currentTime).toBe(1);
});

test('Space activates a focused button but plays from a focused crop handle', async () => {
  const [video] = await loadClip();
  const loop = screen.getByRole('switch', { name: 'Loop selection' });
  act(() => loop.focus());
  // Not cancelled, so the browser goes on to toggle the switch itself.
  expect(press(' ', {}, loop)).toBe(true);
  expect(played).toEqual([]);

  Object.defineProperty(video, 'videoWidth', { value: 640 });
  Object.defineProperty(video, 'videoHeight', { value: 360 });
  fireEvent.loadedMetadata(video);
  const crop = screen.getByRole('button', { name: 'Move crop' });
  act(() => crop.focus());
  expect(press(' ', {}, crop)).toBe(false);
  expect(played).toEqual([video]);
});

test('keys pressed outside the editor leave the clip alone', async () => {
  const [video] = await loadClip();
  const preset = screen.getByRole('button', { name: /Drop a video/ });
  act(() => preset.focus());
  video.currentTime = 10;
  press(' ', {}, preset);
  press('i', {}, preset);
  press('Enter', { ctrlKey: true }, preset);
  expect(played).toEqual([]);
  expect(startField().value).toBe('0:00.00');
  await act(async () => {});
  expect(exported()).toBe(false);

  // Opening a file is page-wide.
  press('O', { ctrlKey: true }, preset);
  expect(dialog.open).toHaveBeenCalledTimes(1);
});

test('the hint row lists the shortcuts under the timeline', async () => {
  await loadClip();
  const hints = screen.getByRole('list', { name: 'Keyboard shortcuts' });
  expect(hints).toHaveTextContent('Space play');
  expect(hints).toHaveTextContent('I/O start/end');
  expect(hints).toHaveTextContent('Ctrl+Enter convert');
  expect(hints).toHaveTextContent('Ctrl+O open');
});
