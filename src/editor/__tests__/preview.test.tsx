// Copied from mgck's __tests__/discordgifs-preview.test.tsx: the preview maths and orientation
// tests as they are, and its redraw tests moved from simple mode's preset previews to the
// output preview, on a directly played source and on a proxy.
import { act, fireEvent, screen } from '@testing-library/react';

import Editor from '../Editor';
import {
  canvasResolution,
  cropSourceRect,
  drawOutputFrame,
  fitToBox,
} from '../lib/output-preview';
import type { Orientation } from '../lib/output-preview';
import {
  clipBackend,
  mock2dCanvas,
  openClip,
  renderWithListeners,
  resetTauri,
  sourceInfo,
  ffmpegFound,
} from './tauri';

vi.mock('@tauri-apps/api/core', async () => (await import('./tauri')).core);
vi.mock('@tauri-apps/api/event', async () => (await import('./tauri')).event);
vi.mock('@tauri-apps/api/webview', async () => (await import('./tauri')).webview);
vi.mock('@tauri-apps/plugin-dialog', async () => (await import('./tauri')).dialog);

const frame = { width: 1280, height: 720 };

describe('preview maths', () => {
  test('the kept source is the crop in source pixels, or the whole frame', () => {
    expect(cropSourceRect(undefined, frame)).toEqual({
      x: 0,
      y: 0,
      width: 1280,
      height: 720,
    });
    expect(
      cropSourceRect({ x: 0.25, y: 0.5, width: 0.5, height: 0.25 }, frame)
    ).toEqual({ x: 320, y: 360, width: 640, height: 180 });
  });

  test('contain fits the long side to the box', () => {
    const wide = cropSourceRect(undefined, frame);
    expect(fitToBox(wide, 160, 'contain')).toEqual({
      source: wide,
      display: { width: 160, height: 90 },
    });
    const tall = cropSourceRect(undefined, { width: 720, height: 1280 });
    expect(fitToBox(tall, 160, 'contain').display).toEqual({
      width: 90,
      height: 160,
    });
  });

  test('cover fills the box from the centre of the source, as a round avatar', () => {
    const kept = cropSourceRect({ x: 0.1, y: 0, width: 0.8, height: 1 }, frame);
    expect(fitToBox(kept, 80, 'cover')).toEqual({
      source: { x: 128 + (1024 - 720) / 2, y: 0, width: 720, height: 720 },
      display: { width: 80, height: 80 },
    });
  });

  test('canvas pixels follow the screen but never exceed the output', () => {
    const kept = cropSourceRect(undefined, frame);
    // A 140 px output shown in a 160 px box stays 140 pixels across.
    const sticker = fitToBox(kept, 160, 'contain');
    expect(
      canvasResolution(sticker.display, sticker.source, kept, 140, 2)
    ).toEqual({ width: 140, height: 79 });
    // A 512 px avatar shown at 80 px has pixels to spare for a 2x screen.
    const avatar = fitToBox(kept, 80, 'cover');
    expect(
      canvasResolution(avatar.display, avatar.source, kept, 512, 2)
    ).toEqual({ width: 160, height: 160 });
  });
});

describe('rotated and flipped preview frames', () => {
  /**
   * Where each corner of the source lands on the canvas, read off the
   * transform the frame was drawn with, as FFmpeg's transpose and flips
   * would place it.
   */
  type Matrix = [number, number, number, number, number, number];
  type DrawArgs = [number, number, number, number, number, number, number, number];
  function corners(
    orientation: Orientation,
    canvas: { width: number; height: number }
  ) {
    const transforms: number[][] = [];
    const draws: number[][] = [];
    const context = {
      clearRect: vi.fn(),
      setTransform: (...m: number[]) => transforms.push(m),
      drawImage: (_: unknown, ...args: number[]) => draws.push(args),
    } as unknown as CanvasRenderingContext2D;
    drawOutputFrame(
      context,
      {} as CanvasImageSource,
      { x: 0, y: 0, width: 1280, height: 720 },
      canvas,
      orientation
    );
    // Reset, clear, draw transform, reset.
    const [a, b, c, d, e, f] = transforms[1] as Matrix;
    const [, , , , , , width, height] = draws[0] as number[] as DrawArgs;
    const place = (x: number, y: number) => [
      Math.round(a * x + c * y + e),
      Math.round(b * x + d * y + f),
    ];
    expect(transforms[transforms.length - 1]).toEqual([1, 0, 0, 1, 0, 0]);
    return {
      topLeft: place(0, 0),
      topRight: place(width, 0),
      bottomLeft: place(0, height),
    };
  }
  const flat = {
    rotation: 0,
    flipHorizontal: false,
    flipVertical: false,
  } as const;
  const wide = { width: 160, height: 90 };
  const tall = { width: 90, height: 160 };

  test('upright draws the source as it is', () => {
    expect(corners(flat, wide)).toEqual({
      topLeft: [0, 0],
      topRight: [160, 0],
      bottomLeft: [0, 90],
    });
  });

  test('a right turn puts the top left corner at the top right', () => {
    expect(corners({ ...flat, rotation: 90 }, tall)).toEqual({
      topLeft: [90, 0],
      topRight: [90, 160],
      bottomLeft: [0, 0],
    });
  });

  test('a left turn puts the top left corner at the bottom left', () => {
    expect(corners({ ...flat, rotation: 270 }, tall)).toEqual({
      topLeft: [0, 160],
      topRight: [0, 0],
      bottomLeft: [90, 160],
    });
  });

  test('a half turn and both flips land the same way', () => {
    const half = corners({ ...flat, rotation: 180 }, wide);
    expect(half.topLeft).toEqual([160, 90]);
    expect(
      corners({ ...flat, flipHorizontal: true, flipVertical: true }, wide)
    ).toEqual(half);
  });

  test('flips mirror after the turn, as hflip follows transpose', () => {
    expect(corners({ ...flat, flipHorizontal: true }, wide).topLeft).toEqual([
      160, 0,
    ]);
    expect(corners({ ...flat, flipVertical: true }, wide).topLeft).toEqual([
      0, 90,
    ]);
    // transpose=1 then hflip: the source top left ends at the canvas top left.
    expect(
      corners({ ...flat, rotation: 90, flipHorizontal: true }, tall)
    ).toEqual({ topLeft: [0, 0], topRight: [0, 160], bottomLeft: [90, 0] });
  });
});

describe('output preview', () => {
  let drawImage: ReturnType<typeof vi.fn>;
  beforeEach(() => {
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation(() => 0);
    drawImage = mock2dCanvas();
  });
  afterEach(() => {
    resetTauri();
    vi.restoreAllMocks();
  });

  const drawing = () =>
    screen.getByRole('img', { name: 'Output preview drawing' });
  const press = (name: string) =>
    fireEvent.click(screen.getByRole('button', { name }));

  test.each([
    ['the source', 'source', { width: 1280, height: 720 }],
    ['a proxy', 'proxy', { width: 640, height: 360 }],
  ] as const)(
    'draws the crop from %s and redraws when it changes',
    async (_, preview, frame) => {
      clipBackend(sourceInfo({ width: 1280, height: 720 }), preview);
      await renderWithListeners(<Editor ffmpeg={ffmpegFound} />);
      const video = await openClip('C:/videos/clip.mp4', frame);
      // The crop is a fraction of the frame, so a proxy's pixels give the same picture.
      const k = frame.width / 1280;
      press('1:1');
      expect(drawImage).toHaveBeenLastCalledWith(
        video,
        280 * k,
        0,
        720 * k,
        720 * k,
        0,
        0,
        expect.any(Number),
        expect.any(Number)
      );
      expect([drawing().style.width, drawing().style.height]).toEqual([
        '474px',
        '474px',
      ]);

      press('Free');
      press('Reset crop');
      expect(drawImage).toHaveBeenLastCalledWith(
        video,
        0,
        0,
        1280 * k,
        720 * k,
        0,
        0,
        expect.any(Number),
        expect.any(Number)
      );
    }
  );

  test('a new frame redraws the preview', async () => {
    clipBackend(sourceInfo());
    await renderWithListeners(<Editor ffmpeg={ffmpegFound} />);
    const video = await openClip('C:/videos/clip.mp4', { width: 1280, height: 720 });
    drawImage.mockClear();
    await act(async () => {
      fireEvent.seeked(video);
    });
    expect(drawImage).toHaveBeenCalledTimes(1);
  });
});
