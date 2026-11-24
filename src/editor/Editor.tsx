import clsx from 'clsx';
import { useCallback, useState } from 'react';

import Inspector from './Inspector';
import ShortcutsOverlay from './ShortcutsOverlay';
import SourceEditor from './SourceEditor';
import StatusBar from './StatusBar';
import Toolbar from './Toolbar';
import { PlaceholderColumn } from './Viewer';
import { useDestination } from './useDestination';
import { useExport } from './useExport';
import { useMemoryEstimate } from './useMemoryEstimate';
import { fileName, useSource } from './useSource';
import { useClipShortcuts } from './useClipShortcuts';
import type { Tool } from '../ipc/probe';
import {
  OutputSettings,
  defaultOutputSettings,
  forNewClip,
} from './lib/output-settings';
import { ClipCrop, ClipTrim } from './lib/types';

/**
 * The app's window: toolbar, the editor's main area, status bar. It fills the window and never
 * scrolls as a page; only the inspector's body scrolls. It holds the open source's trim, crop and
 * settings; the source and its preview come from `useSource`, exports and their results from
 * `useExport`, and where they are saved from `useDestination`.
 */
export default function Editor({ ffmpeg }: { ffmpeg: Tool }) {
  const [settings, setSettings] = useState<OutputSettings>(
    defaultOutputSettings
  );
  const [edit, setEdit] = useState<{ trim?: ClipTrim; crop?: ClipCrop }>({});
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  // A new clip starts with no trim or crop, upright, forward and uncaptioned.
  const onOpen = useCallback(() => {
    setEdit({});
    setSettings(forNewClip);
  }, []);
  const { state, dragging, chooseFile, close, unplayable } = useSource(onOpen);
  const { job, results, start, cancel } = useExport();
  const open = state.stage === 'open' ? state : null;
  const saveTo = useDestination(open ? open.path : null);
  const memory = useMemoryEstimate(
    open ? { info: open.info, ...edit, settings } : null
  );
  const closeShortcuts = useCallback(() => setShortcutsOpen(false), []);

  useClipShortcuts(
    {
      openFile: () => void chooseFile(),
      showShortcuts: () => setShortcutsOpen(true),
    },
    { enabled: !shortcutsOpen }
  );

  return (
    <main
      className={clsx(
        'grid h-dvh min-w-0 grid-rows-[44px_minmax(0,1fr)_26px] overflow-hidden bg-canvas',
        dragging && 'outline outline-2 -outline-offset-2 outline-accent'
      )}
    >
      <h1 className='sr-only'>video-to-gif</h1>
      <Toolbar
        path={state.stage === 'empty' ? null : state.path}
        info={open && open.info}
        closeDisabled={job !== null}
        onOpen={() => void chooseFile()}
        onClose={close}
      />
      {open ? (
        <SourceEditor
          // A new clip starts with fresh media and export state.
          key={open.id}
          source={open}
          trim={edit.trim}
          crop={edit.crop}
          settings={settings}
          results={results}
          memory={memory}
          onSettingsChange={setSettings}
          onTrimChange={(trim) => setEdit((current) => ({ ...current, trim }))}
          onCropChange={(crop) => setEdit((current) => ({ ...current, crop }))}
          onUnplayable={unplayable}
          job={job}
          saveTo={saveTo}
          onExport={start}
          onCancel={cancel}
        />
      ) : (
        <div className='grid min-h-0 grid-cols-[minmax(0,1fr)_340px]'>
          <PlaceholderColumn>
            {state.stage === 'probing' ? (
              <p aria-busy='true'>Opening {fileName(state.path)}…</p>
            ) : (
              <>
                {state.stage === 'failed' && (
                  <p role='alert' className='max-w-2xl font-medium text-danger'>
                    {state.message}
                  </p>
                )}
                <p className='text-lg font-medium'>
                  Drop a video or GIF anywhere in this window
                </p>
                <p className='text-dim'>
                  Anything your ffmpeg can read, at any size. The file is
                  opened in place, never copied.
                </p>
                <button
                  type='button'
                  className='mt-1 flex h-[38px] items-center gap-2.5 rounded-[7px] bg-accent px-5 text-[13.5px] font-semibold text-accent-fg hover:bg-accent-hover'
                  onClick={() => void chooseFile()}
                >
                  Open video…
                  <span
                    aria-hidden
                    className='rounded border border-[#5a2c08]/45 px-1 font-mono text-2xs text-[#5a2c08]'
                  >
                    Ctrl O
                  </span>
                </button>
              </>
            )}
          </PlaceholderColumn>
          <Inspector
            results={results}
            job={job}
            onCancel={cancel}
            output={
              <p className='px-6 py-10 text-center text-dim'>
                Output settings appear here once a video is open.
              </p>
            }
          />
        </div>
      )}
      <StatusBar
        ffmpeg={ffmpeg}
        preview={open && open.preview}
        memory={memory}
        onShowShortcuts={() => setShortcutsOpen(true)}
      />
      {shortcutsOpen && <ShortcutsOverlay onClose={closeShortcuts} />}
    </main>
  );
}
