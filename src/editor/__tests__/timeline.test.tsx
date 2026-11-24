// The timeline dock on a long recording: zoom, scrolling, the ruler's ticks, frame-exact in and
// out points with the mouse, and the denser thumbnails a zoomed view asks for.
import { act, fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';

import VideoTrimmer, { type TrimmerSource } from '../VideoTrimmer';
import { ClipTrim } from '../lib/types';
import { rulerTicks } from '../lib/timeline';
import { backend, invoked, resetTauri } from './tauri';

vi.mock('@tauri-apps/api/core', async () => (await import('./tauri')).core);

/** Frames a second; frame k starts at k / rate. */
const rate = 30;
const frame = 1 / rate;

/** A 40-minute recording at 30 fps. */
const source: TrimmerSource = {
  path: 'C:/videos/stream.mp4',
  src: 'asset://stream.mp4',
  duration: 2400,
  width: 1920,
  height: 1080,
  frameRate: rate,
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
  backend({ make_thumbnails: () => new Promise(() => {}) });
  vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue();
  vi.spyOn(window, 'requestAnimationFrame').mockImplementation(() => 0);
});
afterEach(() => {
  resetTauri();
  vi.restoreAllMocks();
});

function Harness() {
  const [trim, setTrim] = useState<ClipTrim | undefined>();
  return (
    <>
      <VideoTrimmer
        source={source}
        trim={trim}
        onCropChange={vi.fn()}
        mode='source'
        onModeChange={vi.fn()}
        output={() => null}
        disabled={false}
        onChange={setTrim}
        onUnplayable={vi.fn()}
      />
      <output data-testid='trim'>
        {trim ? `${trim.start}-${trim.end}` : 'whole'}
      </output>
    </>
  );
}

const trimValues = () =>
  screen.getByTestId('trim').textContent!.split('-').map(Number);
const player = () => document.querySelector('video')!;

/**
 * The ruler and track measure 1000 px, the width the dock assumes before layout, so at the
 * deepest zoom (48 frames across) a frame is 1000 / 48 px.
 */
const width = 1000;
const timeline = () => {
  const element = screen.getByRole('group', { name: 'Trim timeline' });
  vi.spyOn(element, 'getBoundingClientRect').mockReturnValue({
    left: 0,
    width,
  } as DOMRect);
  return element;
};
const ruler = () => screen.getByTestId('timeline-ruler');
const press = (target: Element, x: number) => {
  fireEvent.pointerDown(target, { clientX: x, button: 0, pointerId: 1 });
  fireEvent.pointerUp(target, { clientX: x, pointerId: 1 });
};
const drag = (target: Element, fromX: number, toX: number) => {
  fireEvent.pointerDown(target, { clientX: fromX, button: 0, pointerId: 1 });
  fireEvent.pointerMove(target, { clientX: toX, pointerId: 1 });
  fireEvent.pointerUp(target, { clientX: toX, pointerId: 1 });
};
const zoom = () => screen.getByRole('slider', { name: 'Timeline zoom' });
const zoomFully = () => fireEvent.change(zoom(), { target: { value: '1000' } });
/** Where the playhead line sits on the track, as a percentage. */
const playheadAt = () =>
  parseFloat(
    screen.getByRole('slider', { name: 'Playhead' }).parentElement!.style.left
  );
/** Where a handle sits on the track, in pixels. */
const handleX = (name: 'In point' | 'Out point') =>
  (parseFloat(screen.getByRole('slider', { name }).style.left) / 100) * width;

test('on a 40-minute source the deepest zoom shows single frames, and In and Out land on exact frames by mouse', () => {
  render(<Harness />);
  timeline();

  // Fitted, the ruler counts in minutes.
  expect(Number(ruler().dataset.step)).toBe(300);
  press(ruler(), 500);
  expect(player().currentTime).toBe(1200);

  zoomFully();
  // 48 frames across 1000 px, with the playhead kept in the middle where it was.
  expect(playheadAt()).toBeCloseTo(50);
  expect(Number(ruler().dataset.minorStep)).toBeCloseTo(frame, 9);
  const ticks = [...ruler().querySelectorAll<HTMLElement>('[data-tick]')]
    .map((tick) => Number(tick.dataset.time))
    .sort((a, b) => a - b);
  expect(ticks.length).toBeGreaterThanOrEqual(48);
  for (let i = 1; i < ticks.length; i++)
    expect(ticks[i]! - ticks[i - 1]!).toBeCloseTo(frame, 9);

  // Three frames right of the playhead, then Set in.
  const pixelsPerFrame = width / 48;
  press(ruler(), 500 + 3 * pixelsPerFrame);
  expect(player().currentTime).toBe(36003 / rate);
  fireEvent.click(screen.getByText('Set in'));
  expect(trimValues()[0]).toBe(36003 / rate);

  // Twelve frames right of the old playhead, then Set out.
  press(ruler(), 500 + 12 * pixelsPerFrame);
  fireEvent.click(screen.getByText('Set out'));
  expect(trimValues()).toEqual([36003 / rate, 36012 / rate]);
  expect(screen.getByLabelText('In')).toHaveValue('20:00.10');
  expect(screen.getByLabelText('Out')).toHaveValue('20:00.40');
  expect(screen.getByText('0.30 s')).toBeInTheDocument();

  // Handles dragged by a little under and over whole frames snap to frames.
  const inX = handleX('In point');
  expect(inX).toBeCloseTo(500 + 3 * pixelsPerFrame);
  drag(
    screen.getByRole('slider', { name: 'In point' }),
    inX,
    inX - 1.9 * pixelsPerFrame
  );
  expect(trimValues()[0]).toBe(36001 / rate);
  const outX = handleX('Out point');
  drag(
    screen.getByRole('slider', { name: 'Out point' }),
    outX,
    outX + 1.3 * pixelsPerFrame
  );
  expect(trimValues()).toEqual([36001 / rate, 36013 / rate]);
  // Releasing the out handle returns the playhead to the in point, and the view stays put.
  expect(player().currentTime).toBe(36001 / rate);
  expect(handleX('Out point')).toBeCloseTo(500 + 13 * pixelsPerFrame);
});

test('Ctrl+wheel zooms at the playhead and the wheel scrolls a zoomed timeline', () => {
  render(<Harness />);
  const surface = timeline();
  press(ruler(), 250);
  expect(player().currentTime).toBe(600);
  expect(playheadAt()).toBeCloseTo(25);

  // Unzoomed, the wheel is left alone.
  expect(fireEvent.wheel(surface, { deltaY: 100 })).toBe(true);
  expect((zoom() as HTMLInputElement).value).toBe('0');

  expect(fireEvent.wheel(surface, { deltaY: -500, ctrlKey: true })).toBe(false);
  expect(Number((zoom() as HTMLInputElement).value)).toBeGreaterThan(0);
  expect(playheadAt()).toBeCloseTo(25);

  // A 100 px wheel scrolls a tenth of the 1000 px track.
  expect(fireEvent.wheel(surface, { deltaY: 100 })).toBe(false);
  expect(playheadAt()).toBeCloseTo(15);
  fireEvent.wheel(surface, { deltaX: -200, deltaY: 10 });
  expect(playheadAt()).toBeCloseTo(35);

  // Zooming back out with the slider fits the whole clip.
  fireEvent.change(zoom(), { target: { value: '0' } });
  expect(playheadAt()).toBeCloseTo(25);
});

test('the view follows playback past its edge and scrolls with its scrollbar', () => {
  render(<Harness />);
  timeline();
  press(ruler(), 500);
  zoomFully();
  const video = player();
  video.currentTime = 1205;
  fireEvent.timeUpdate(video);
  // Paged so the playhead is a tenth of the way in.
  expect(playheadAt()).toBeCloseTo(10);

  const bar = screen.getByTestId('timeline-scrollbar');
  vi.spyOn(bar, 'getBoundingClientRect').mockReturnValue({
    left: 0,
    width,
  } as DOMRect);
  // Pressing beside the thumb centres the view there; dragging scrolls on.
  fireEvent.pointerDown(bar, { clientX: 0, button: 0, pointerId: 1 });
  expect(Number(ruler().querySelector<HTMLElement>('[data-tick]')!.dataset.time)).toBeLessThan(1);
  fireEvent.pointerMove(bar, { clientX: 250, pointerId: 1 });
  fireEvent.pointerUp(bar, { clientX: 250, pointerId: 1 });
  const first = Math.min(
    ...[...ruler().querySelectorAll<HTMLElement>('[data-tick]')].map((tick) =>
      Number(tick.dataset.time)
    )
  );
  expect(first).toBeGreaterThan(599);
  expect(first).toBeLessThan(601);
});

test('a zoomed view asks make_thumbnails for a denser strip of the view', async () => {
  backend({
    make_thumbnails: async (args) => {
      const prefix = args.range ? 'range' : 'whole';
      return Array.from({ length: 24 }, (_, i) => `/t/${prefix}-${i + 1}.jpg`);
    },
  });
  render(<Harness />);
  timeline();
  await screen.findByTestId('timeline-thumbnails');
  expect(screen.queryByTestId('timeline-range-thumbnails')).toBeNull();
  expect(invoked('make_thumbnails')).toHaveLength(1);

  press(ruler(), 500);
  zoomFully();
  const strip = await screen.findByTestId(
    'timeline-range-thumbnails',
    {},
    { timeout: 2000 }
  );
  const [, args] = invoked('make_thumbnails').at(-1)!;
  const range = (args as { range: { start: number; end: number } }).range;
  // The view, 1.6 s around the playhead, plus half a view either side.
  expect(range.start).toBeCloseTo(1198.4);
  expect(range.end).toBeCloseTo(1201.6);
  // Twelve of its 24 thumbnails cover the view.
  const cells = [...strip.children] as HTMLElement[];
  expect(cells.length).toBeGreaterThanOrEqual(12);
  expect(cells.length).toBeLessThanOrEqual(14);
  expect(cells[0]!.style.backgroundImage).toContain('range-');

  // Zoomed back out, the whole-source strip is enough.
  fireEvent.change(zoom(), { target: { value: '0' } });
  await act(async () => {});
  expect(screen.queryByTestId('timeline-range-thumbnails')).toBeNull();
});

test('ruler labels read m:ss, h:mm:ss past an hour, and m:ss.cc between seconds', () => {
  const labels = (start: number, span: number) =>
    rulerTicks({ start, span }, 1000, frame).labelled.map((tick) => tick.label);
  expect(labels(0, 2400).slice(0, 3)).toEqual(['0:00', '5:00', '10:00']);
  expect(labels(3500, 400)).toContain('1:00:00');
  expect(labels(1200, 1.6).slice(0, 2)).toEqual(['20:00.00', '20:00.17']);
});
