// Copied from mgck's __tests__/discordgifs-trim.test.tsx. The trimmer takes its source's facts
// from the probe and its thumbnails from ffmpeg; simple mode's sticker checks and the ffmpeg.wasm
// manager's command test stay in mgck.
import { act, fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';

import VideoTrimmer, { type TrimmerSource } from '../VideoTrimmer';
import { ClipTrim } from '../lib/types';
import { backend, resetTauri } from './tauri';

vi.mock('@tauri-apps/api/core', async () => (await import('./tauri')).core);

const frame = 1 / 30;

const source: TrimmerSource = {
  path: 'C:/videos/clip.mp4',
  src: 'asset://clip.mp4',
  duration: 60,
  width: 1280,
  height: 720,
  frameRate: 30,
};

// jsdom has no PointerEvent, so pointer events would lose their coordinates.
class TestPointerEvent extends MouseEvent {
  pointerId: number;
  constructor(type: string, init: PointerEventInit = {}) {
    super(type, init);
    this.pointerId = init.pointerId ?? 1;
  }
}
beforeAll(() => {
  window.PointerEvent = TestPointerEvent as unknown as typeof PointerEvent;
  Element.prototype.setPointerCapture = () => {};
});

beforeEach(() => {
  // Thumbnails stay pending unless a test supplies them, as while ffmpeg runs.
  backend({ make_thumbnails: () => new Promise(() => {}) });
  vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue();
  // The playback frame loop is exercised through timeupdate instead.
  vi.spyOn(window, 'requestAnimationFrame').mockImplementation(() => 0);
});
afterEach(() => {
  resetTauri();
  vi.restoreAllMocks();
});

/** Renders the trimmer with real trim state, as the editor holds it. */
function Harness({
  initialTrim,
  disabled = false,
  onUnplayable = vi.fn(),
}: {
  initialTrim?: ClipTrim;
  disabled?: boolean;
  onUnplayable?: () => void;
}) {
  const [trim, setTrim] = useState<ClipTrim | undefined>(initialTrim);
  return (
    <>
      <VideoTrimmer
        source={source}
        trim={trim}
        onCropChange={vi.fn()}
        disabled={disabled}
        onChange={setTrim}
        onUnplayable={onUnplayable}
      />
      <output data-testid='trim'>
        {trim ? `${trim.start}-${trim.end}` : 'whole'}
      </output>
    </>
  );
}

const trimText = () => screen.getByTestId('trim').textContent!;
const trimValues = () => trimText().split('-').map(Number) as [number, number];
const player = (container: HTMLElement) => container.querySelector('video')!;
const type = (label: 'Start' | 'End', value: string) => {
  const field = screen.getByLabelText(label);
  fireEvent.change(field, { target: { value } });
  fireEvent.keyDown(field, { key: 'Enter' });
  return field;
};
/** The strip measures 600 px, so 10 px is one second of the 60-second clip. */
const strip = () => {
  const element = screen.getByRole('group', { name: 'Trim timeline' });
  vi.spyOn(element, 'getBoundingClientRect').mockReturnValue({
    left: 0,
    width: 600,
  } as DOMRect);
  return element;
};
const drag = (target: Element, fromX: number, toX: number) => {
  fireEvent.pointerDown(target, { clientX: fromX, button: 0, pointerId: 1 });
  fireEvent.pointerMove(target, { clientX: toX, pointerId: 1 });
  fireEvent.pointerUp(target, { clientX: toX, pointerId: 1 });
};

test('Whole clip clears the selection', () => {
  render(<Harness />);
  expect(screen.getByText('Whole clip')).toBeDisabled();
  type('Start', '0:20');
  expect(trimText()).toBe('20-60');
  expect(screen.getByText('40.00 s')).toBeInTheDocument();
  fireEvent.click(screen.getByText('Whole clip'));
  expect(trimText()).toBe('whole');
});

test('typed times accept m:ss.cc or seconds, ignore invalid input and seek to the edge', () => {
  const { container } = render(<Harness />);
  expect(screen.getByLabelText('Start')).toHaveValue('0:00.00');
  expect(screen.getByLabelText('End')).toHaveValue('1:00.00');
  type('Start', '0:12.50');
  type('End', '15.25');
  expect(trimText()).toBe('12.5-15.25');
  expect(player(container).currentTime).toBe(15.25);
  expect(screen.getByLabelText('End')).toHaveValue('0:15.25');
  expect(screen.getByText('2.75 s')).toBeInTheDocument();
  const start = type('Start', 'soon');
  expect(start).toHaveValue('0:12.50');
  expect(trimText()).toBe('12.5-15.25');
  fireEvent.change(start, { target: { value: '0:14' } });
  fireEvent.keyDown(start, { key: 'Escape' });
  fireEvent.blur(start);
  expect(start).toHaveValue('0:12.50');
});

test('typed times are clamped inside the clip with the start at least one frame before the end', () => {
  render(<Harness initialTrim={{ start: 10, end: 20 }} />);
  type('Start', '0:30');
  let [start, end] = trimValues();
  expect(end).toBe(20);
  expect(start).toBeCloseTo(20 - frame);
  type('End', '0:05');
  [start, end] = trimValues();
  expect(end - start).toBeCloseTo(frame);
  type('End', '2:00');
  expect(trimValues()[1]).toBe(60);
  type('Start', '0');
  expect(trimText()).toBe('whole');
});

test('Set start and Set end use the playhead, clamped the same way', () => {
  const { container } = render(<Harness />);
  const timeline = strip();
  fireEvent.pointerDown(timeline, { clientX: 300, button: 0 });
  fireEvent.pointerUp(timeline, { clientX: 300 });
  expect(player(container).currentTime).toBe(30);
  fireEvent.click(screen.getByText('Set end'));
  expect(trimText()).toBe('0-30');
  fireEvent.click(screen.getByText('Set start'));
  const [start, end] = trimValues();
  expect(end).toBe(30);
  expect(start).toBeCloseTo(30 - frame);
});

test('handles nudge by a frame, or a second with Shift, and never cross', () => {
  render(<Harness initialTrim={{ start: 10, end: 20 }} />);
  const start = screen.getByRole('slider', { name: 'Selection start' });
  const end = screen.getByRole('slider', { name: 'Selection end' });
  expect(start).toHaveAttribute('tabindex', '0');
  fireEvent.keyDown(start, { key: 'ArrowRight' });
  expect(trimValues()[0]).toBeCloseTo(10 + frame);
  fireEvent.keyDown(start, { key: 'ArrowLeft', shiftKey: true });
  expect(trimValues()[0]).toBeCloseTo(9 + frame);
  fireEvent.keyDown(start, { key: 'End' });
  expect(trimValues()[0]).toBeCloseTo(20 - frame);
  fireEvent.keyDown(end, { key: 'Home' });
  const [s, e] = trimValues();
  expect(e).toBeGreaterThan(s);
  expect(e - s).toBeCloseTo(frame);
  fireEvent.keyDown(end, { key: 'End' });
  expect(trimValues()[1]).toBe(60);
  expect(end).toHaveAttribute('aria-valuetext', '1:00.00');
});

test('dragging the handles and the span edits the selection inside the clip', () => {
  const { container } = render(
    <Harness initialTrim={{ start: 10, end: 20 }} />
  );
  strip();
  const start = screen.getByRole('slider', { name: 'Selection start' });
  const end = screen.getByRole('slider', { name: 'Selection end' });
  const span = container.querySelector('[data-part="span"]')!;
  drag(start, 100, 50);
  expect(trimText()).toBe('5-20');
  expect(player(container).currentTime).toBe(5);
  drag(end, 200, 0);
  expect(trimValues()[1]).toBeCloseTo(5 + frame);
  drag(end, 0, 400);
  expect(trimText()).toBe('5-40');
  // Releasing the end handle returns the playhead to the start.
  expect(player(container).currentTime).toBe(5);
  drag(span, 200, 450);
  expect(trimText()).toBe('25-60');
  drag(span, 500, 100);
  expect(trimText()).toBe('0-35');
  // A press on the span that does not move it only seeks.
  drag(span, 300, 300);
  expect(trimText()).toBe('0-35');
  expect(player(container).currentTime).toBe(30);
});

test('the playhead seeks on click or drag and pauses playback while scrubbing', () => {
  const { container } = render(<Harness />);
  const video = player(container);
  const timeline = strip();
  fireEvent.play(video);
  const grip = screen.getByRole('slider', { name: 'Playhead' });
  fireEvent.pointerDown(grip, { clientX: 0, button: 0 });
  expect(video.pause).toHaveBeenCalled();
  fireEvent.pointerMove(timeline, { clientX: 420 });
  expect(video.currentTime).toBe(42);
  fireEvent.pause(video);
  fireEvent.pointerUp(timeline, { clientX: 420 });
  expect(video.play).toHaveBeenCalledTimes(1);
  expect(trimText()).toBe('whole');
  fireEvent.keyDown(grip, { key: 'ArrowRight', shiftKey: true });
  expect(video.currentTime).toBe(43);
});

test('loop selection is on by default and keeps playback inside the selection', () => {
  const { container } = render(<Harness initialTrim={{ start: 2, end: 5 }} />);
  const video = player(container);
  expect(screen.getByLabelText('Loop selection')).toBeChecked();
  fireEvent.click(screen.getByRole('button', { name: 'Play' }));
  expect(video.currentTime).toBe(2);
  expect(video.play).toHaveBeenCalled();
  fireEvent.play(video);
  expect(screen.getByRole('button', { name: 'Pause' })).toBeInTheDocument();
  video.currentTime = 5;
  fireEvent.timeUpdate(video);
  expect(video.currentTime).toBe(2);
  expect(video.pause).not.toHaveBeenCalled();
  fireEvent.click(screen.getByLabelText('Loop selection'));
  video.currentTime = 5.1;
  fireEvent.timeUpdate(video);
  expect(video.pause).toHaveBeenCalled();
  expect(video.currentTime).toBe(5);
});

test('transport steps frames, shows the time and plays sound only when asked', () => {
  const { container } = render(<Harness />);
  const video = player(container);
  expect(video.muted).toBe(true);
  expect(screen.getByLabelText('Sound')).not.toBeChecked();
  video.currentTime = 1;
  fireEvent.click(screen.getByLabelText('Next frame'));
  expect(video.pause).toHaveBeenCalled();
  expect(video.currentTime).toBeCloseTo(1 + frame);
  fireEvent.click(screen.getByLabelText('Previous frame'));
  fireEvent.click(screen.getByLabelText('Previous frame'));
  expect(video.currentTime).toBeCloseTo(1 - frame);
  expect(screen.getByText('0:00.97')).toBeInTheDocument();
  expect(screen.getByText(/\/ 1:00\.00/)).toBeInTheDocument();
  fireEvent.click(screen.getByLabelText('Sound'));
  expect(video.muted).toBe(false);
});

test('a preview the player cannot decode is reported, for the editor to make a proxy', () => {
  const onUnplayable = vi.fn();
  const { container } = render(<Harness onUnplayable={onUnplayable} />);
  fireEvent.error(player(container));
  expect(onUnplayable).toHaveBeenCalledTimes(1);
  // Video it can't decode beside audio it can loads with no picture.
  fireEvent.loadedMetadata(player(container));
  expect(onUnplayable).toHaveBeenCalledTimes(2);
});

test('trim controls cannot change a queued conversion', () => {
  render(<Harness initialTrim={{ start: 10, end: 20 }} disabled />);
  expect(screen.getByLabelText('Start')).toBeDisabled();
  expect(screen.getByLabelText('End')).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Play' })).toBeDisabled();
  expect(screen.getByText('Set start')).toBeDisabled();
  const start = screen.getByRole('slider', { name: 'Selection start' });
  expect(start).toHaveAttribute('tabindex', '-1');
  fireEvent.keyDown(start, { key: 'ArrowRight' });
  drag(start, 100, 300);
  expect(trimText()).toBe('10-20');
});

test('the timeline shows ffmpeg-made thumbnails, or a plain bar when they fail', async () => {
  backend({
    make_thumbnails: async () => ['/temp/thumbs-1/thumb-01.jpg', '/temp/thumbs-1/thumb-02.jpg'],
  });
  const { unmount } = render(<Harness />);
  const thumbnails = await screen.findByTestId('timeline-thumbnails');
  expect(thumbnails.firstElementChild).toHaveStyle(
    'background-image: url("asset:///temp/thumbs-1/thumb-01.jpg")'
  );
  unmount();

  backend({
    make_thumbnails: async () => {
      throw { kind: 'thumbnailsFailed', reason: 'no frames' };
    },
  });
  render(<Harness />);
  await act(async () => {});
  expect(screen.queryByTestId('timeline-thumbnails')).not.toBeInTheDocument();
  expect(screen.getByLabelText('Start')).toBeEnabled();
});
