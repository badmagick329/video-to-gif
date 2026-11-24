// Copied from mgck's __tests__/discordgifs-advanced-mode.test.tsx (the editor's controls), the
// settings-number tests of discordgifs-advanced-commands.test.ts and the Made this session test
// of discordgifs-output.test.tsx. Dropped with what they covered: the large-file warning and
// frame-memory limits (ffmpeg.wasm's), the mode switch (simple mode is not in v0), and the
// ffmpeg command tests (export builds commands in Rust).
import { act, fireEvent, render, screen, within } from '@testing-library/react';

import Editor from '../Editor';
import SessionResults from '../SessionResults';
import {
  AdvancedSettings,
  croppedSourceSize,
  defaultAdvancedSettings,
  forNewClip,
  frameCount,
  isLargeGif,
  outputSeconds,
  outputSize,
  rotate,
  soundDropReason,
  speedSteps,
} from '../lib/advanced';
import { renderAdvancedEditor } from './harness';
import {
  clipBackend,
  drop,
  mock2dCanvas,
  openClip,
  outputDetails as details,
  renderWithListeners,
  resetTauri,
  sourceInfo,
} from './tauri';

vi.mock('@tauri-apps/api/core', async () => (await import('./tauri')).core);
vi.mock('@tauri-apps/api/event', async () => (await import('./tauri')).event);
vi.mock('@tauri-apps/api/webview', async () => (await import('./tauri')).webview);
vi.mock('@tauri-apps/plugin-dialog', async () => (await import('./tauri')).dialog);

const settings = (patch: Partial<AdvancedSettings> = {}): AdvancedSettings => ({
  ...defaultAdvancedSettings,
  ...patch,
});

beforeEach(() => {
  vi.spyOn(window, 'requestAnimationFrame').mockImplementation(() => 0);
  mock2dCanvas();
});
afterEach(() => {
  resetTauri();
  vi.restoreAllMocks();
});

const press = (name: string) =>
  fireEvent.click(screen.getByRole('button', { name }));
const convertButton = () => screen.getByRole('button', { name: /^Convert to/ });
const convert = () => act(async () => fireEvent.click(convertButton()));

/** Opens a 4-second 1280 × 720 clip with sound, played directly or through a 640 × 360 proxy. */
async function openEditor(preview: 'source' | 'proxy' = 'source') {
  clipBackend(sourceInfo(), preview);
  const view = await renderWithListeners(<Editor />);
  await openClip(
    'C:/videos/one.mp4',
    preview === 'proxy' ? { width: 640, height: 360 } : { width: 1280, height: 720 }
  );
  return view;
}

describe('Advanced mode', () => {
  test('holds one clip: opening another replaces it', async () => {
    await openEditor();
    expect(screen.getByRole('button', { name: 'Remove one.mp4' })).toBeInTheDocument();
    await drop('C:/videos/two.mp4');
    expect(
      await screen.findByRole('button', { name: 'Remove two.mp4' })
    ).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Remove one.mp4' })).toBeNull();
    expect(screen.getAllByRole('button', { name: /^Remove / })).toHaveLength(1);
  });

  test('converts the ticked file types, with Made this session empty until export exists', async () => {
    const { request } = renderAdvancedEditor({ info: sourceInfo() });
    const results = screen.getByRole('region', { name: 'Made this session' });
    expect(within(results).getByText(/Nothing yet/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('checkbox', { name: /MP4/ }));
    await convert();
    expect(request()).toMatchObject({
      path: 'C:/videos/clip.mp4',
      settings: { fileTypes: ['gif', 'mp4'], fps: 15 },
      outputSeconds: 4,
    });
  });

  test('the app’s Convert stops before exporting and says so', async () => {
    await openEditor();
    expect(convertButton()).toHaveTextContent('Convert to GIF');
    await convert();
    expect(screen.getByRole('status')).toHaveTextContent('Converting is not built yet.');
  });

  test('no file type ticked leaves nothing to convert', async () => {
    await openEditor();
    fireEvent.click(screen.getByRole('checkbox', { name: /GIF/ }));
    expect(
      screen.getByRole('button', { name: 'Pick a file type to save as' })
    ).toBeDisabled();
  });
});

describe.each(['source', 'proxy'] as const)(
  'Advanced mode rotate, flip, speed and direction, playing the %s',
  (preview) => {
    test('a quarter turn swaps the output size, the preview and the crop readout', async () => {
      await openEditor(preview);
      expect(details()).toHaveTextContent('480 × 270');
      const drawing = screen.getByRole('img', { name: 'Output preview drawing' });
      expect([drawing.style.width, drawing.style.height]).toEqual(['300px', '169px']);

      press('Rotate right');
      expect(details()).toHaveTextContent('480 × 854');
      expect(screen.getByText('Turned 90°')).toBeInTheDocument();
      expect([drawing.style.width, drawing.style.height]).toEqual(['169px', '300px']);

      // 16:9 is picked in output pixels, so on the turned clip the box is a
      // 405 × 720 strip of the source and reads as 720 × 405.
      press('16:9');
      expect(screen.getByText('Crop 720 × 405 px')).toBeInTheDocument();
      expect(details()).toHaveTextContent('480 × 270');

      press('Rotate right');
      expect(screen.getByText('Turned 180°')).toBeInTheDocument();
      expect(details()).toHaveTextContent('480 × 270');
    });

    test('width presets wider than the crop are disabled, not shown as chosen', async () => {
      await openEditor(preview);
      const preset = (name: string) => screen.getByRole('button', { name });
      expect(preset('480')).toHaveAttribute('aria-pressed', 'true');

      // A 9:16 box on a 1280 × 720 frame is about 405 px wide.
      press('9:16');
      expect(preset('480')).toBeDisabled();
      expect(preset('480')).toHaveAttribute('aria-pressed', 'false');
      expect(preset('640')).toBeDisabled();
      expect(preset('400')).toBeEnabled();
    });

    test('a locked ratio is re-fitted when a turn swaps it', async () => {
      await openEditor(preview);
      press('16:9');
      // The whole 16:9 frame is no crop at all.
      expect(screen.getByText('Full frame. Drag a corner to crop.')).toBeInTheDocument();

      press('Rotate left');
      expect(screen.getByText('Crop 720 × 405 px')).toBeInTheDocument();
      expect(details()).toHaveTextContent('480 × 270');

      press('Rotate right');
      expect(screen.getByText('Full frame. Drag a corner to crop.')).toBeInTheDocument();
    });

    test('the preview labels what it cannot play, and plays at the chosen speed', async () => {
      const { container } = await openEditor(preview);
      press('Rotate right');
      press('Flip ↔');
      fireEvent.change(screen.getByRole('slider', { name: 'Speed' }), {
        target: { value: '10' },
      });
      press('Boomerang');

      expect(screen.getByRole('button', { name: 'Flip ↔' })).toHaveAttribute('aria-pressed', 'true');
      expect(screen.getByRole('button', { name: 'Boomerang' })).toHaveAttribute('aria-pressed', 'true');
      // The editor plays at the chosen speed, so the live preview does too.
      expect(container.querySelector('video')!.playbackRate).toBe(2);
      const figure = screen.getByRole('figure', { name: 'Output preview' });
      expect(figure).toHaveTextContent('live · 2×');
      expect(figure).toHaveTextContent(
        'The preview plays forward. Boomerang is applied when you convert.'
      );
      // A 4 s selection at 2× is 2 s, played there and back.
      expect(details()).toHaveTextContent('4.0 s');
      expect(details()).toHaveTextContent('60 frames');
    });

    test('Keep sound says when the speed or direction drops the sound', async () => {
      await openEditor(preview);
      fireEvent.click(screen.getByText('Quality & looping'));
      fireEvent.click(screen.getByRole('switch', { name: 'Keep sound in MP4' }));
      expect(screen.queryByText(/^No sound/)).toBeNull();

      press('Reverse');
      expect(screen.getByText('No sound in Reverse.')).toBeInTheDocument();

      press('Forward');
      fireEvent.change(screen.getByRole('slider', { name: 'Speed' }), {
        target: { value: '12' },
      });
      expect(
        screen.getByText('No sound at 3×; it keeps up from 0.5× to 2×.')
      ).toBeInTheDocument();
    });

    test('a new clip starts upright and forward, keeping the speed', async () => {
      await openEditor(preview);
      press('Rotate right');
      press('Flip ↕');
      press('Reverse');
      fireEvent.change(screen.getByRole('slider', { name: 'Speed' }), {
        target: { value: '8' },
      });

      await drop('C:/videos/two.mp4');
      await screen.findByRole('button', { name: 'Remove two.mp4' });
      expect(screen.queryByText(/^Turned/)).toBeNull();
      expect(screen.getByRole('button', { name: 'Flip ↕' })).toHaveAttribute('aria-pressed', 'false');
      expect(screen.getByRole('button', { name: 'Forward' })).toHaveAttribute('aria-pressed', 'true');
      expect(screen.getByRole('slider', { name: 'Speed' })).toHaveAttribute('aria-valuetext', '1.5×');
    });
  }
);

test('the request carries every edit', async () => {
  const { request } = renderAdvancedEditor({ info: sourceInfo() });
  press('Rotate right');
  press('Flip ↔');
  fireEvent.change(screen.getByRole('slider', { name: 'Speed' }), {
    target: { value: '10' },
  });
  press('Boomerang');
  fireEvent.click(screen.getByText('Quality & looping'));
  fireEvent.change(screen.getByRole('slider', { name: 'GIF colours' }), {
    target: { value: '64' },
  });
  fireEvent.change(screen.getByRole('combobox', { name: 'Dithering' }), {
    target: { value: 'bayer' },
  });
  fireEvent.change(screen.getByRole('combobox', { name: 'Loop' }), {
    target: { value: '2' },
  });
  fireEvent.change(screen.getByRole('slider', { name: 'WebP quality' }), {
    target: { value: '55' },
  });
  fireEvent.click(screen.getByRole('switch', { name: 'Keep sound in MP4' }));
  fireEvent.change(screen.getByRole('slider', { name: 'Frame rate' }), {
    target: { value: '24' },
  });

  await convert();
  expect(request()).toMatchObject({
    settings: {
      rotation: 90,
      flipHorizontal: true,
      flipVertical: false,
      speed: 2,
      direction: 'boomerang',
      gifColors: 64,
      gifDither: 'bayer',
      plays: 2,
      webpQuality: 55,
      fps: 24,
    },
    width: 480,
    keepSound: true,
    outputSeconds: 4,
  });
});

test('a source without sound offers no Keep sound', async () => {
  const { request } = renderAdvancedEditor({ info: sourceInfo({ hasAudio: false }) });
  fireEvent.click(screen.getByText('Quality & looping'));
  expect(screen.queryByRole('switch', { name: 'Keep sound in MP4' })).toBeNull();
  await convert();
  expect(request().keepSound).toBe(false);
});

describe('rotate, flip, speed and direction numbers', () => {
  test('quarter turns swap the output size; a half turn does not', () => {
    const portrait = { width: 1080, height: 1920 };
    expect(outputSize(480, portrait, 0)).toEqual({ width: 480, height: 854, native: 1080 });
    expect(outputSize(480, portrait, 90)).toEqual({ width: 480, height: 270, native: 1920 });
    expect(outputSize('original', portrait, 270)).toMatchObject({ width: 1920, height: 1080 });
    expect(outputSize(480, portrait, 180)).toEqual(outputSize(480, portrait, 0));
  });

  test('rotate steps a quarter either way and wraps', () => {
    expect(rotate(0, 'right')).toBe(90);
    expect(rotate(270, 'right')).toBe(0);
    expect(rotate(0, 'left')).toBe(270);
    expect(rotate(90, 'left')).toBe(0);
  });

  test('speed changes the length and Boomerang doubles it', () => {
    expect(outputSeconds(4, { speed: 1, direction: 'forward' })).toBe(4);
    expect(outputSeconds(4, { speed: 2, direction: 'forward' })).toBe(2);
    expect(outputSeconds(4, { speed: 2, direction: 'boomerang' })).toBe(4);
    expect(outputSeconds(4, { speed: 0.5, direction: 'reverse' })).toBe(8);
  });

  test('a new clip starts upright, forward and uncaptioned but keeps speed and sizes', () => {
    expect(
      forNewClip(
        settings({
          rotation: 90,
          flipHorizontal: true,
          flipVertical: true,
          direction: 'boomerang',
          topText: 'TOP',
          bottomText: 'BOTTOM',
          captionSize: 20,
          speed: 2,
          width: 320,
        })
      )
    ).toEqual(settings({ speed: 2, width: 320, captionSize: 20 }));
  });

  test('the sound note says why sound is dropped', () => {
    expect(soundDropReason({ speed: 1, direction: 'forward' })).toBeNull();
    expect(soundDropReason({ speed: 0.5, direction: 'forward' })).toBeNull();
    expect(soundDropReason({ speed: 2, direction: 'forward' })).toBeNull();
    expect(soundDropReason({ speed: 3, direction: 'forward' })).toBe(
      'No sound at 3×; it keeps up from 0.5× to 2×.'
    );
    expect(soundDropReason({ speed: 1, direction: 'reverse' })).toBe('No sound in Reverse.');
    expect(soundDropReason({ speed: 1, direction: 'boomerang' })).toBe('No sound in Boomerang.');
  });

  test('speed steps are Gif Studio steps, 0.25× to 3×', () => {
    expect(speedSteps).toEqual([0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1, 1.25, 1.5, 1.75, 2, 2.5, 3]);
  });
});

describe('Advanced mode output numbers', () => {
  const frame = { width: 1920, height: 1080 };

  test('defaults: 480 px wide, 15 fps, GIF only, 256 colours, forever', () => {
    expect(defaultAdvancedSettings).toEqual({
      width: 480,
      fps: 15,
      speed: 1,
      direction: 'forward',
      rotation: 0,
      flipHorizontal: false,
      flipVertical: false,
      topText: '',
      bottomText: '',
      captionSize: 12,
      fileTypes: ['gif'],
      gifColors: 256,
      gifDither: 'sierra2_4a',
      plays: 0,
      webpQuality: 80,
      keepSound: false,
    });
  });

  test('width is even, capped at the crop, and the height follows scale=-2', () => {
    const full = croppedSourceSize(undefined, frame);
    expect(outputSize(480, full)).toEqual({ width: 480, height: 270, native: 1920 });
    expect(outputSize(481, full).width).toBe(480);
    expect(outputSize('original', full)).toMatchObject({ width: 1920, height: 1080 });

    // A 301-pixel crop truncates like FFmpeg's crop filter and caps at 300.
    const narrow = croppedSourceSize({ x: 0, y: 0, width: 301 / 1920, height: 0.5 }, frame);
    expect(narrow).toEqual({ width: 301, height: 540 });
    expect(outputSize(640, narrow)).toEqual({ width: 300, height: 538, native: 300 });
  });

  test('frame count and the big-GIF warning follow Gif Studio', () => {
    expect(frameCount(5, 15)).toBe(75);
    expect(frameCount(2.2, 15)).toBe(33);
    expect(isLargeGif({ width: 480, height: 270 }, 300)).toBe(false);
    expect(isLargeGif({ width: 480, height: 270 }, 301)).toBe(true);
  });
});

describe('Made this session', () => {
  test('shows why a file type failed beside the failure', () => {
    render(
      <SessionResults
        results={[
          { id: 'a', fileType: 'gif', failed: true, reason: 'This video is AV1.' },
          { id: 'b', fileType: 'webp', failed: true },
        ]}
      />
    );
    const rows = screen.getAllByRole('listitem');
    expect(rows.map((row) => row.textContent)).toEqual([
      'GIF conversion failedThis video is AV1.',
      'WebP conversion failed',
    ]);
  });
});
