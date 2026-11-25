// The start state: the window with nothing open, drop anywhere, Recent, and failed opens.
import { act, fireEvent, screen, within } from '@testing-library/react';

import Editor from '../Editor';
import type { RecentSource } from '../../ipc/preview';
import {
  backend,
  core,
  dialog,
  drag,
  drop,
  estimateAnswers,
  toolsFound,
  invoked,
  memoryFits,
  mock2dCanvas,
  renderWithListeners,
  resetTauri,
  sourceInfo,
} from './tauri';

vi.mock('@tauri-apps/api/core', async () => (await import('./tauri')).core);
vi.mock('@tauri-apps/api/event', async () => (await import('./tauri')).event);
vi.mock(
  '@tauri-apps/api/webview',
  async () => (await import('./tauri')).webview
);
vi.mock(
  '@tauri-apps/plugin-dialog',
  async () => (await import('./tauri')).dialog
);

const info = sourceInfo({ duration: 12, width: 1920, height: 1080 });

const now = new Date(2026, 10, 24, 18, 0).getTime();
const hour = 3600_000;

const recent: RecentSource[] = [
  {
    path: 'D:\\Videos\\Fancams\\concert.mkv',
    duration: 724,
    openedAt: now - 2 * hour,
  },
  {
    path: 'C:\\Users\\you\\Downloads\\stage.mp4',
    duration: 288,
    openedAt: now - 26 * hour,
  },
  {
    path: 'C:\\Users\\you\\Videos\\OBS\\recording.mkv',
    duration: 2477,
    openedAt: now - 20 * 24 * hour,
  },
];

/** A backend that opens any path as a playable source and lists `list` as Recent. */
function startBackend(list: RecentSource[] = recent) {
  backend({
    recent_sources: async () => list,
    forget_recent_source: async () => {},
    probe_source: async () => info,
    open_source: async () => 'source',
    make_thumbnails: () => new Promise(() => {}),
    saved_destination: async () => null,
    estimate_export_memory: async () => memoryFits,
    estimate_output_size: estimateAnswers({}),
    cancel_size_estimate: async () => {},
  });
}

const dropZone = () =>
  screen
    .getByText('Drop a video or GIF anywhere in this window')
    .closest('div.border-dashed')!;
const recentList = () =>
  screen.getByRole('navigation', { name: 'Recent videos' });

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(now);
  vi.spyOn(window, 'requestAnimationFrame').mockImplementation(() => 0);
  mock2dCanvas();
});
afterEach(() => {
  vi.useRealTimers();
  resetTauri();
  vi.restoreAllMocks();
});

test('a fresh launch shows the empty window: Open, the drop zone, Recent, and what carries over', async () => {
  startBackend();
  await renderWithListeners(<Editor tools={toolsFound} />);

  const toolbar = screen.getByRole('banner');
  expect(
    within(toolbar).getByRole('button', { name: /^Open/ })
  ).toBeInTheDocument();
  expect(toolbar).toHaveTextContent('No video open');

  const viewer = screen.getByRole('region', { name: 'Viewer' });
  expect(viewer).toHaveTextContent(
    'Drop a video or GIF anywhere in this window'
  );
  expect(viewer).toHaveTextContent(
    'Anything your ffmpeg can read, at any size. The file is opened in place, never copied.'
  );
  expect(
    within(viewer).getByRole('button', { name: /^Open video…/ })
  ).toBeInTheDocument();

  const rows = within(recentList()).getAllByRole('listitem');
  expect(rows.map((row) => row.textContent)).toEqual([
    'concert.mkv12:04D:\\Videos\\FancamsToday 16:00',
    'stage.mp44:48C:\\Users\\you\\DownloadsYesterday 16:00',
    'recording.mkv41:17C:\\Users\\you\\Videos\\OBS4 Nov',
  ]);

  const inspector = screen.getByRole('complementary', { name: 'Inspector' });
  expect(inspector).toHaveTextContent(
    'Output settings appear here once a video is open.'
  );
  expect(inspector).toHaveTextContent('Carried over: GIF · 480 px · 15 fps');
  // Nothing exported yet, so no tabs: the board's bare inspector.
  expect(within(inspector).queryByRole('tab')).not.toBeInTheDocument();

  const timeline = screen.getByRole('region', { name: 'Timeline' });
  expect(timeline).toHaveTextContent('Timeline');
  expect(within(timeline).queryByRole('button')).not.toBeInTheDocument();
});

test('a Recent entry opens through the same commands as a dialog pick', async () => {
  startBackend();
  await renderWithListeners(<Editor tools={toolsFound} />);

  fireEvent.click(
    within(recentList()).getByRole('button', { name: /^stage\.mp4/ })
  );
  await act(async () => {});

  const path = 'C:\\Users\\you\\Downloads\\stage.mp4';
  expect(invoked('probe_source')).toEqual([['probe_source', { path }]]);
  expect(invoked('open_source')).toEqual([['open_source', { path, info }]]);
  expect(document.querySelector('video')).toHaveAttribute(
    'src',
    `asset://${path}`
  );
  expect(
    screen.getByRole('button', { name: 'Close stage.mp4' })
  ).toBeInTheDocument();
});

test('closing a clip shows Recent again, loaded afresh', async () => {
  startBackend();
  await renderWithListeners(<Editor tools={toolsFound} />);
  expect(invoked('recent_sources')).toHaveLength(1);

  await drop('C:\\videos\\new.mp4');
  core.invoke.mockClear();
  await act(async () =>
    fireEvent.click(screen.getByRole('button', { name: 'Close new.mp4' }))
  );

  expect(invoked('recent_sources')).toHaveLength(1);
  expect(recentList()).toBeInTheDocument();
});

test('the remove button takes an entry off Recent and leaves the rest', async () => {
  startBackend();
  await renderWithListeners(<Editor tools={toolsFound} />);

  fireEvent.click(
    screen.getByRole('button', { name: 'Remove stage.mp4 from Recent' })
  );
  await act(async () => {});

  expect(invoked('forget_recent_source')).toEqual([
    ['forget_recent_source', { path: 'C:\\Users\\you\\Downloads\\stage.mp4' }],
  ]);
  expect(within(recentList()).getAllByRole('listitem')).toHaveLength(2);
  expect(within(recentList()).queryByText('stage.mp4')).not.toBeInTheDocument();
  // Removing opens nothing.
  expect(invoked('probe_source')).toEqual([]);
});

test('with nothing in Recent, the list is not shown', async () => {
  startBackend([]);
  await renderWithListeners(<Editor tools={toolsFound} />);
  expect(
    screen.queryByRole('navigation', { name: 'Recent videos' })
  ).not.toBeInTheDocument();
});

test('an entry whose file has gone since the list loaded says so', async () => {
  backend({
    recent_sources: async () => recent,
    probe_source: async ({ path }) => {
      throw { kind: 'notFound', path };
    },
  });
  await renderWithListeners(<Editor tools={toolsFound} />);

  fireEvent.click(
    within(recentList()).getByRole('button', { name: /^concert\.mkv/ })
  );
  expect(await screen.findByRole('alert')).toHaveTextContent(
    'There is no file at D:\\Videos\\Fancams\\concert.mkv.'
  );
  expect(document.querySelector('video')).toBeNull();
});

test('dragging over the window lights up the drop zone, and a drop opens the file', async () => {
  startBackend();
  await renderWithListeners(<Editor tools={toolsFound} />);
  expect(dropZone()).not.toHaveClass('border-accent');

  await drag('enter', 'C:\\videos\\clip.mp4');
  expect(dropZone()).toHaveClass('border-accent');
  await drag('leave');
  expect(dropZone()).not.toHaveClass('border-accent');

  await drag('enter', 'C:\\videos\\clip.mp4');
  await drag('over', 'C:\\videos\\clip.mp4');
  await drop('C:\\videos\\clip.mp4');
  expect(invoked('open_source')).toEqual([
    ['open_source', { path: 'C:\\videos\\clip.mp4', info }],
  ]);
  expect(
    screen.getByRole('button', { name: 'Close clip.mp4' })
  ).toBeInTheDocument();
});

test('with a video open, dragging highlights the viewer and a drop replaces the video', async () => {
  startBackend();
  await renderWithListeners(<Editor tools={toolsFound} />);
  await drop('C:\\videos\\first.mp4');
  expect(screen.queryByText(/^Drop to open/)).not.toBeInTheDocument();

  await drag('enter', 'C:\\videos\\second.mp4');
  expect(
    screen.getByText('Drop to open it in place of first.mp4')
  ).toBeInTheDocument();
  await drop('C:\\videos\\second.mp4');

  expect(screen.queryByText(/^Drop to open/)).not.toBeInTheDocument();
  expect(
    screen.getByRole('button', { name: 'Close second.mp4' })
  ).toBeInTheDocument();
  expect(document.querySelector('video')).toHaveAttribute(
    'src',
    'asset://C:\\videos\\second.mp4'
  );
});

test('Open video in the drop zone opens the file dialog', async () => {
  startBackend();
  dialog.open.mockResolvedValue(null);
  await renderWithListeners(<Editor tools={toolsFound} />);
  fireEvent.click(screen.getByRole('button', { name: /^Open video…/ }));
  expect(dialog.open).toHaveBeenCalledTimes(1);
});
