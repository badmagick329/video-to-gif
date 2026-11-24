// The advanced editor with the state the app holds for it, for tests that read the export
// request Convert builds. Mock the Tauri modules with ./tauri first.
import { fireEvent, render } from '@testing-library/react';
import { useState } from 'react';

import AdvancedEditor from '../AdvancedEditor';
import { defaultAdvancedSettings } from '../lib/advanced';
import type { ClipCrop, ClipTrim, ExportRequest } from '../lib/types';
import type { SourceInfo } from '../../ipc/probe';
import { clipBackend, proxyPath } from './tauri';

export function renderAdvancedEditor({
  info,
  preview = 'source',
  frame = { width: info.width, height: info.height },
}: {
  info: SourceInfo;
  preview?: 'source' | 'proxy';
  /** The player's decoded frame: the proxy's size when it plays one. */
  frame?: { width: number; height: number };
}) {
  const onExport = vi.fn(async (request: ExportRequest) => void request);
  const path = 'C:/videos/clip.mp4';
  function Harness() {
    const [trim, setTrim] = useState<ClipTrim>();
    const [crop, setCrop] = useState<ClipCrop>();
    const [settings, setSettings] = useState(defaultAdvancedSettings);
    return (
      <AdvancedEditor
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
        onSettingsChange={setSettings}
        onTrimChange={setTrim}
        onCropChange={setCrop}
        onRemove={() => {}}
        onUnplayable={() => {}}
        onExport={onExport}
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
  /** The request the latest Convert sent. */
  const request = () => onExport.mock.calls.at(-1)![0];
  return { ...view, video, onExport, request };
}
