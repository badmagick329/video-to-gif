// The source editor with the state the app holds for it, for tests that read the export request
// Export builds. Mock the Tauri modules with ./tauri first.
import { fireEvent, render } from '@testing-library/react';
import { useState } from 'react';

import SourceEditor from '../SourceEditor';
import { defaultOutputSettings } from '../lib/output-settings';
import type { ClipCrop, ClipTrim, ExportRequest } from '../lib/types';
import type { MemoryEstimate } from '../../ipc/export';
import type { SourceInfo } from '../../ipc/probe';
import { clipBackend, proxyPath } from './tauri';

export function renderSourceEditor({
  info,
  preview = 'source',
  frame = { width: info.width, height: info.height },
  memory = null,
}: {
  info: SourceInfo;
  preview?: 'source' | 'proxy';
  /** The player's decoded frame: the proxy's size when it plays one. */
  frame?: { width: number; height: number };
  /** The frame-memory estimate the window would pass down. */
  memory?: MemoryEstimate | null;
}) {
  const onExport = vi.fn(
    async (request: ExportRequest, destination: string) => {
      void request;
      return { text: `Saved to ${destination}.`, failed: false };
    }
  );
  const path = 'C:/videos/clip.mp4';
  function Harness() {
    const [trim, setTrim] = useState<ClipTrim>();
    const [crop, setCrop] = useState<ClipCrop>();
    const [settings, setSettings] = useState(defaultOutputSettings);
    return (
      <SourceEditor
        source={{
          id: 1,
          path,
          info,
          preview: {
            stage: 'ready',
            kind: preview,
            src: `asset://${preview === 'proxy' ? proxyPath : path}`,
          },
        }}
        trim={trim}
        crop={crop}
        settings={settings}
        results={[]}
        memory={memory}
        onSettingsChange={setSettings}
        onTrimChange={setTrim}
        onCropChange={setCrop}
        onUnplayable={() => {}}
        job={null}
        saveTo={{
          destination: { folder: 'C:/videos', from: 'source' },
          error: '',
          choose: async () => {},
          saveBesideSource: async () => {},
        }}
        onExport={onExport}
        onCancel={() => {}}
      />
    );
  }
  clipBackend(info, preview);
  const view = render(<Harness />);
  const video = view.container.querySelector('video')!;
  Object.defineProperties(video, {
    videoWidth: { value: frame.width, configurable: true },
    videoHeight: { value: frame.height, configurable: true },
    readyState: { value: 4, configurable: true },
  });
  fireEvent.loadedMetadata(video);
  fireEvent.loadedData(video);
  /** The request the latest Export sent. */
  const request = () => onExport.mock.calls.at(-1)![0];
  return { ...view, video, onExport, request };
}
