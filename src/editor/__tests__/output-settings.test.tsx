// Copied from mgck's __tests__/discordgifs-advanced-mode.test.tsx (the editor's controls) and the
// settings-number tests of discordgifs-advanced-commands.test.ts. Dropped with what they covered:
// the large-file warning and frame-memory limits (ffmpeg.wasm's), the mode switch (simple mode is
// not in v0), and the ffmpeg command tests (export builds commands in Rust). Exporting, its
// progress and the This session list are tested in export.test.tsx.
import { act, fireEvent, screen, within } from '@testing-library/react';

import Editor from '../Editor';
import {
  OutputSettings,
  croppedSourceSize,
  defaultOutputSettings,
  forNewClip,
  frameCount,
  outputSeconds,
  outputSize,
  rotate,
  soundDropReason,
  speedSteps,
  usableGifEncoder,
} from '../lib/output-settings';
import { renderSourceEditor } from './harness';
import {
  clipBackend,
  drop,
  mock2dCanvas,
  openClip,
  estimatedSize,
  exportButton,
  outputSize as details,
  renderWithListeners,
  resetTauri,
  sourceInfo,
  toolsFound,
  toolsWithGifski,
} from './tauri';

vi.mock('@tauri-apps/api/core', async () => (await import('./tauri')).core);
vi.mock('@tauri-apps/api/event', async () => (await import('./tauri')).event);
vi.mock('@tauri-apps/api/webview', async () => (await import('./tauri')).webview);
vi.mock('@tauri-apps/plugin-dialog', async () => (await import('./tauri')).dialog);

const settings = (patch: Partial<OutputSettings> = {}): OutputSettings => ({
  ...defaultOutputSettings,
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
const runExport = () => act(async () => fireEvent.click(exportButton()));

/** Opens a 4-second 1280 × 720 clip with sound, played directly or through a 640 × 360 proxy. */
async function openEditor(preview: 'source' | 'proxy' = 'source') {
  clipBackend(sourceInfo(), preview);
  const view = await renderWithListeners(<Editor tools={toolsFound} />);
  await openClip(
    'C:/videos/one.mp4',
    preview === 'proxy' ? { width: 640, height: 360 } : { width: 1280, height: 720 }
  );
  return view;
}

describe('Output settings', () => {
  test('holds one clip: opening another replaces it', async () => {
    await openEditor();
    expect(screen.getByRole('button', { name: 'Close one.mp4' })).toBeInTheDocument();
    await drop('C:/videos/two.mp4');
    expect(
      await screen.findByRole('button', { name: 'Close two.mp4' })
    ).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Close one.mp4' })).toBeNull();
    expect(screen.getAllByRole('button', { name: /^Close / })).toHaveLength(1);
  });

  test('exports the ticked file types, with This session empty until something is made', async () => {
    const { request } = renderSourceEditor({ info: sourceInfo() });
    fireEvent.click(screen.getByRole('tab', { name: /^Exports/ }));
    const results = screen.getByRole('region', { name: 'This session' });
    expect(within(results).getByText(/Nothing yet/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('tab', { name: 'Output' }));

    fireEvent.click(screen.getByRole('button', { name: 'MP4' }));
    expect(exportButton()).toHaveAccessibleName('Export GIF + MP4');
    await runExport();
    expect(request()).toMatchObject({
      path: 'C:/videos/clip.mp4',
      info: sourceInfo(),
      settings: { fileTypes: ['gif', 'mp4'], fps: 15 },
    });
  });

  test('the toolbar and status bar describe the open source', async () => {
    await openEditor('proxy');
    expect(screen.getByText('one.mp4')).toHaveAttribute('title', 'C:/videos/one.mp4');
    const chips = within(screen.getByRole('list', { name: 'Source details' }))
      .getAllByRole('listitem')
      .map((chip) => chip.textContent);
    expect(chips).toEqual(['1280 × 720', 'H.264', '30 fps', '0:04.00', 'AAC']);
    expect(screen.getByText('ffmpeg 9.0.1 on PATH')).toBeInTheDocument();
    expect(screen.getByText('Playing a preview copy')).toBeInTheDocument();
  });

  test('the viewer starts side by side and remembers the mode it was left in', async () => {
    const view = await openEditor();
    const mode = (name: string) => screen.getByRole('button', { name });
    expect(mode('Side by side')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByLabelText('Move crop')).toBeInTheDocument();
    expect(screen.getByRole('figure', { name: 'Output preview' })).toBeInTheDocument();

    // Output keeps the source's video mounted, since the preview draws from it.
    press('Output');
    expect(screen.getByRole('figure', { name: 'Output preview' })).toBeInTheDocument();
    expect(view.container.querySelector('video')).toBeInTheDocument();
    press('Source');
    expect(screen.queryByRole('figure', { name: 'Output preview' })).toBeNull();

    view.unmount();
    await openEditor();
    expect(mode('Source')).toHaveAttribute('aria-pressed', 'true');
  });

  test('no file type ticked leaves nothing to export', async () => {
    await openEditor();
    fireEvent.click(screen.getByRole('button', { name: 'GIF' }));
    expect(screen.getByRole('button', { name: 'Export' })).toBeDisabled();
    expect(estimatedSize()).toHaveTextContent('Pick at least one format');
  });
});

describe.each(['source', 'proxy'] as const)(
  'Rotate, flip, speed and direction, playing the %s',
  (preview) => {
    test('a quarter turn swaps the output size, the preview and the crop readout', async () => {
      await openEditor(preview);
      expect(details()).toHaveTextContent('480 × 270');
      const drawing = screen.getByRole('img', { name: 'Output preview drawing' });
      expect([drawing.style.width, drawing.style.height]).toEqual(['474px', '267px']);

      press('Rotate right');
      expect(details()).toHaveTextContent('480 × 854');
      expect(screen.getByText('Turned 90°')).toBeInTheDocument();
      expect([drawing.style.width, drawing.style.height]).toEqual(['291px', '518px']);

      // 16:9 is picked in output pixels, so on the turned clip the box is a
      // 405 × 720 strip of the source and reads as 720 × 405.
      press('16:9');
      expect(screen.getByText('Source · crop 720 × 405')).toBeInTheDocument();
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
      expect(screen.getByText('Source · full frame, drag a corner to crop')).toBeInTheDocument();

      press('Rotate left');
      expect(screen.getByText('Source · crop 720 × 405')).toBeInTheDocument();
      expect(details()).toHaveTextContent('480 × 270');

      press('Rotate right');
      expect(screen.getByText('Source · full frame, drag a corner to crop')).toBeInTheDocument();
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
      expect(figure).toHaveTextContent('Output · 480 × 854 · 15 fps · plays at 2×');
      expect(figure).toHaveTextContent(
        'The preview plays forward. Boomerang is applied when you export.'
      );
      // A 4 s selection at 2× is 2 s, played there and back.
      expect(estimatedSize()).toHaveTextContent('60 frames · 4.0 s');
    });

    test('Keep sound says when the speed or direction drops the sound', async () => {
      await openEditor(preview);
      press('MP4');
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
      await screen.findByRole('button', { name: 'Close two.mp4' });
      expect(screen.queryByText(/^Turned/)).toBeNull();
      expect(screen.getByRole('button', { name: 'Flip ↕' })).toHaveAttribute('aria-pressed', 'false');
      expect(screen.getByRole('button', { name: 'Forward' })).toHaveAttribute('aria-pressed', 'true');
      expect(screen.getByRole('slider', { name: 'Speed' })).toHaveAttribute('aria-valuetext', '1.5×');
    });
  }
);

test('the request carries every edit', async () => {
  const { request } = renderSourceEditor({ info: sourceInfo() });
  press('Rotate right');
  press('Flip ↔');
  fireEvent.change(screen.getByRole('slider', { name: 'Speed' }), {
    target: { value: '10' },
  });
  press('Boomerang');
  // WebP quality and Keep sound show with their file types.
  expect(screen.queryByRole('slider', { name: 'WebP quality' })).toBeNull();
  press('WebP');
  press('MP4');
  const quality = screen.getByRole('button', { name: 'GIF quality & looping' });
  expect(quality).toHaveAttribute('aria-expanded', 'false');
  expect(quality.closest('h3')).toHaveTextContent('256 colours · Smooth · Forever');
  fireEvent.click(quality);
  fireEvent.change(screen.getByRole('slider', { name: 'Colours' }), {
    target: { value: '64' },
  });
  fireEvent.change(screen.getByRole('combobox', { name: 'Dithering' }), {
    target: { value: 'bayer' },
  });
  fireEvent.change(screen.getByRole('combobox', { name: 'Plays' }), {
    target: { value: '2' },
  });
  fireEvent.change(screen.getByRole('slider', { name: 'WebP quality' }), {
    target: { value: '55' },
  });
  fireEvent.click(screen.getByRole('switch', { name: 'Keep sound in MP4' }));
  fireEvent.change(screen.getByRole('slider', { name: 'Frame rate' }), {
    target: { value: '24' },
  });

  expect(quality.closest('h3')).toHaveTextContent('64 colours · Pattern · Play twice');

  await runExport();
  expect(request()).toMatchObject({
    settings: {
      fileTypes: ['gif', 'webp', 'mp4'],
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
    keepSound: true,
  });
});

test('a source without sound offers no Keep sound', async () => {
  const { request } = renderSourceEditor({ info: sourceInfo({ audioCodec: null }) });
  press('MP4');
  expect(screen.queryByRole('switch', { name: 'Keep sound in MP4' })).toBeNull();
  await runExport();
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

describe('Output numbers', () => {
  const frame = { width: 1920, height: 1080 };

  test('defaults: 480 px wide, 15 fps, GIF only, 256 colours, forever', () => {
    expect(defaultOutputSettings).toEqual({
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
      gifEncoder: 'ffmpeg',
      gifColors: 256,
      gifDither: 'sierra2_4a',
      gifskiQuality: 90,
      gifskiMotionQuality: 90,
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

  test('frame count rounds output seconds at the frame rate', () => {
    expect(frameCount(5, 15)).toBe(75);
    expect(frameCount(2.2, 15)).toBe(33);
  });
});

describe('GIF encoder', () => {
  const encoder = () => screen.getByRole('combobox', { name: 'GIF encoder' });
  const options = () =>
    within(encoder())
      .getAllByRole('option')
      .map((option) => option.textContent);
  const openQuality = () =>
    fireEvent.click(screen.getByRole('button', { name: 'GIF quality & looping' }));

  test('without gifski on PATH the select holds FFmpeg only and the GIF settings are FFmpeg’s', async () => {
    const { request } = renderSourceEditor({ info: sourceInfo() });
    expect(options()).toEqual(['FFmpeg (palette)']);
    expect(encoder()).toHaveValue('ffmpeg');
    openQuality();
    expect(screen.getByRole('slider', { name: 'Colours' })).toBeInTheDocument();
    expect(screen.getByRole('combobox', { name: 'Dithering' })).toBeInTheDocument();
    expect(screen.queryByRole('slider', { name: 'Quality' })).toBeNull();
    await runExport();
    expect(request().settings).toMatchObject({ gifEncoder: 'ffmpeg', gifColors: 256 });
  });

  test('gifski swaps colours and dithering for its quality and motion quality; plays stays', async () => {
    const { request } = renderSourceEditor({ info: sourceInfo(), gifskiFound: true });
    expect(options()).toEqual(['FFmpeg (palette)', 'gifski (high quality)']);
    fireEvent.change(encoder(), { target: { value: 'gifski' } });
    const quality = screen.getByRole('button', { name: 'GIF quality & looping' });
    expect(quality.closest('h3')).toHaveTextContent('Quality 90 · Motion 90 · Forever');
    openQuality();
    expect(screen.queryByRole('slider', { name: 'Colours' })).toBeNull();
    expect(screen.queryByRole('combobox', { name: 'Dithering' })).toBeNull();
    fireEvent.change(screen.getByRole('slider', { name: 'Quality' }), {
      target: { value: '75' },
    });
    fireEvent.change(screen.getByRole('slider', { name: 'Motion quality' }), {
      target: { value: '60' },
    });
    fireEvent.change(screen.getByRole('combobox', { name: 'Plays' }), {
      target: { value: '1' },
    });
    expect(quality.closest('h3')).toHaveTextContent('Quality 75 · Motion 60 · Play once');

    await runExport();
    expect(request().settings).toMatchObject({
      gifEncoder: 'gifski',
      gifskiQuality: 75,
      gifskiMotionQuality: 60,
      plays: 1,
    });
  });

  test('the choice carries over to the next video', async () => {
    clipBackend(sourceInfo());
    await renderWithListeners(<Editor tools={toolsWithGifski} />);
    await openClip('C:/videos/one.mp4', { width: 1280, height: 720 });
    fireEvent.change(encoder(), { target: { value: 'gifski' } });
    await drop('C:/videos/two.mp4');
    await screen.findByRole('button', { name: 'Close two.mp4' });
    expect(encoder()).toHaveValue('gifski');
    expect(screen.getByText('gifski 1.32.0 on PATH')).toBeInTheDocument();
  });

  test('a gifski choice falls back to FFmpeg when gifski is missing', () => {
    const chosen = settings({ gifEncoder: 'gifski' });
    expect(usableGifEncoder(chosen, true)).toBe(chosen);
    expect(usableGifEncoder(chosen, false)).toEqual({ ...chosen, gifEncoder: 'ffmpeg' });
    expect(usableGifEncoder(settings(), false)).toEqual(settings());
  });
});
