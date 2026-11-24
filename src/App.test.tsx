import { render, screen } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';

import App from './App';
import type { FfmpegTools, Tool } from './ipc/probe';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
// The main view listens for drops and proxy progress once ffmpeg is found.
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({ onDragDropEvent: async () => () => {} }),
}));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));

const found = (name: string): Tool => ({
  status: 'found',
  path: `C:\\scripts\\${name}.exe`,
  version: '9.0.1',
});

function reportTools(tools: FfmpegTools) {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === 'find_ffmpeg') return tools;
    throw new Error(`unexpected command ${command}`);
  });
}

afterEach(() => {
  vi.mocked(invoke).mockReset();
});

test('shows the app once ffmpeg and ffprobe are both found', async () => {
  reportTools({ ffmpeg: found('ffmpeg'), ffprobe: found('ffprobe') });
  render(<App />);

  expect(
    await screen.findByRole('heading', { level: 1, name: 'video-to-gif' })
  ).toBeInTheDocument();
  expect(screen.queryByText(/not found on PATH/)).not.toBeInTheDocument();
  expect(invoke).toHaveBeenCalledWith('find_ffmpeg');
});

test('shows the setup message naming ffprobe when it is missing', async () => {
  reportTools({ ffmpeg: found('ffmpeg'), ffprobe: { status: 'missing' } });
  render(<App />);

  expect(
    await screen.findByRole('heading', { name: 'ffmpeg is needed' })
  ).toBeInTheDocument();
  expect(
    screen.getByText('ffprobe was not found on PATH.')
  ).toBeInTheDocument();
  expect(screen.queryByText(/^ffmpeg was not found/)).not.toBeInTheDocument();
  expect(
    screen.queryByRole('heading', { name: 'video-to-gif' })
  ).not.toBeInTheDocument();
});

test('shows the setup message naming ffmpeg when it is missing', async () => {
  reportTools({ ffmpeg: { status: 'missing' }, ffprobe: found('ffprobe') });
  render(<App />);

  expect(
    await screen.findByText('ffmpeg was not found on PATH.')
  ).toBeInTheDocument();
  expect(screen.getByText(/both/)).toHaveTextContent(/on\s+your PATH/);
  expect(
    screen.queryByRole('heading', { name: 'video-to-gif' })
  ).not.toBeInTheDocument();
});

test('shows the setup message when a found ffmpeg does not run', async () => {
  reportTools({
    ffmpeg: {
      status: 'unusable',
      path: 'C:\\tools\\ffmpeg.exe',
      reason: 'could not start',
    },
    ffprobe: found('ffprobe'),
  });
  render(<App />);

  expect(
    await screen.findByText(/ffmpeg at C:\\tools\\ffmpeg\.exe cannot be used/)
  ).toBeInTheDocument();
});
