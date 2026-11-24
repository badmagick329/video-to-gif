import clsx from 'clsx';
import { useCallback, useState } from 'react';

import AdvancedEditor from './AdvancedEditor';
import SessionResults from './SessionResults';
import { fileName, useSource } from './useSource';
import { useClipShortcuts } from './useClipShortcuts';
import {
  AdvancedSettings,
  defaultAdvancedSettings,
  forNewClip,
} from './lib/advanced';
import { ClipCrop, ClipTrim, ExportRequest, SessionResult } from './lib/types';

/**
 * Export arrives with VTG-6 and VTG-7; until then Convert builds the request
 * and stops here.
 */
async function exportStub(request: ExportRequest): Promise<void> {
  void request;
  throw new Error('Converting is not built yet.');
}

/**
 * The app's main view: mgck's Discord GIF page in advanced mode, without the
 * site's chrome. It holds the clip's trim, crop and settings; the source and
 * its preview come from `useSource`.
 */
export default function Editor() {
  const [settings, setSettings] = useState<AdvancedSettings>(
    defaultAdvancedSettings
  );
  const [edit, setEdit] = useState<{ trim?: ClipTrim; crop?: ClipCrop }>({});
  const [results] = useState<SessionResult[]>([]);
  // A new clip starts with no trim or crop, upright, forward and uncaptioned.
  const onOpen = useCallback(() => {
    setEdit({});
    setSettings(forNewClip);
  }, []);
  const { state, dragging, chooseFile, close, unplayable } =
    useSource(onOpen);

  useClipShortcuts({ openFile: () => void chooseFile() }, {});

  return (
    <main className='flex min-h-dvh w-full flex-col bg-background-dg'>
      <h1 className='sr-only'>video-to-gif</h1>
      <div className='flex w-full grow flex-col items-center gap-5 px-2 py-5'>
        <p className='max-w-2xl text-center text-sm text-foreground-dg/80'>
          Convert one clip to GIF, WebP or MP4 with your own width, frame rate
          and quality. Nothing is sized for Discord&apos;s limits.
        </p>
        <button
          type='button'
          className={clsx(
            'flex flex-col items-center gap-4 bg-secondary-dg',
            'rounded-md px-6 py-8 hover:cursor-pointer',
            `shadow-glow-secondary-dg ${dragging && 'scale-110'}`,
            `transition-all`
          )}
          onClick={() => void chooseFile()}
        >
          <span>Drop a video or GIF here, or click to choose one.</span>
          <span className='pb-4'>
            Supported formats:{' '}
            <span className='font-semibold'>
              anything your ffmpeg can read, at any size
            </span>
          </span>
          <span className='text-xs'>
            {state.stage === 'open'
              ? 'Opening another clip replaces this one.'
              : 'Advanced mode converts one clip at a time.'}
          </span>
        </button>
        {state.stage === 'probing' && (
          <p aria-busy='true'>Opening {fileName(state.path)}…</p>
        )}
        {state.stage === 'failed' && (
          <p role='alert' className='max-w-2xl font-semibold text-red-500'>
            {state.message}
          </p>
        )}
        {state.stage === 'open' ? (
          <AdvancedEditor
            // A new clip starts with fresh media and conversion state.
            key={state.id}
            source={state}
            trim={edit.trim}
            crop={edit.crop}
            settings={settings}
            results={results}
            onSettingsChange={setSettings}
            onTrimChange={(trim) => setEdit((current) => ({ ...current, trim }))}
            onCropChange={(crop) => setEdit((current) => ({ ...current, crop }))}
            onRemove={close}
            onUnplayable={unplayable}
            onExport={exportStub}
          />
        ) : (
          results.length > 0 && (
            <div className='w-full max-w-md rounded-md bg-secondary-dg p-4 shadow-glow-secondary-dg'>
              <SessionResults results={results} />
            </div>
          )
        )}
      </div>
    </main>
  );
}
