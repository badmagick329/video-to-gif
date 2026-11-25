import { useCallback, useState } from 'react';

import Inspector from './Inspector';
import ShortcutsOverlay from './ShortcutsOverlay';
import SourceEditor from './SourceEditor';
import StartView from './StartView';
import StatusBar from './StatusBar';
import Toolbar from './Toolbar';
import { PlaceholderColumn } from './Viewer';
import { useDestination } from './useDestination';
import { useExport } from './useExport';
import { useMemoryEstimate } from './useMemoryEstimate';
import { fileName, useSource } from './useSource';
import { useClipShortcuts } from './useClipShortcuts';
import type { Tools } from '../ipc/probe';
import {
  OutputSettings,
  carriedOverSummary,
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
export default function Editor({ tools }: { tools: Tools }) {
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
  const { state, dragging, chooseFile, openPath, close, unplayable } =
    useSource(onOpen);
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
    <main className='grid h-dvh min-w-0 grid-rows-[44px_minmax(0,1fr)_26px] overflow-hidden bg-canvas'>
      <h1 className='sr-only'>video-to-gif</h1>
      <Toolbar
        path={state.stage === 'empty' ? null : state.path}
        info={open && open.info}
        closeDisabled={job !== null}
        onOpen={() => void chooseFile()}
        onClose={close}
      />
      {state.stage === 'open' ? (
        <div className='relative grid min-h-0'>
          <SourceEditor
            // A new clip starts with fresh media and export state.
            key={state.id}
            source={state}
            trim={edit.trim}
            crop={edit.crop}
            settings={settings}
            results={results}
            memory={memory}
            onSettingsChange={setSettings}
            onTrimChange={(trim) =>
              setEdit((current) => ({ ...current, trim }))
            }
            onCropChange={(crop) =>
              setEdit((current) => ({ ...current, crop }))
            }
            onUnplayable={unplayable}
            job={job}
            saveTo={saveTo}
            onExport={start}
            onCancel={cancel}
          />
          {dragging && <DropOverTarget name={fileName(state.path)} />}
        </div>
      ) : (
        <div className='grid min-h-0 grid-cols-[minmax(0,1fr)_340px]'>
          <PlaceholderColumn>
            <StartView
              state={state}
              dragging={dragging}
              onChoose={() => void chooseFile()}
              onOpen={(path) => void openPath(path)}
            />
          </PlaceholderColumn>
          {results.length > 0 || job ? (
            // This session's outputs stay reachable after their clip is closed.
            <Inspector
              results={results}
              job={job}
              onCancel={cancel}
              output={<NothingOpen settings={settings} />}
            />
          ) : (
            <aside
              aria-label='Inspector'
              className='flex min-h-0 min-w-0 flex-col border-l border-line bg-panel'
            >
              <NothingOpen settings={settings} />
            </aside>
          )}
        </div>
      )}
      <StatusBar
        ffmpeg={tools.ffmpeg}
        gifski={tools.gifski}
        preview={open && open.preview}
        memory={memory}
        onShowShortcuts={() => setShortcutsOpen(true)}
      />
      {shortcutsOpen && <ShortcutsOverlay onClose={closeShortcuts} />}
    </main>
  );
}

/**
 * Over the viewer while a file is dragged across the window with a video open: a drop anywhere
 * opens the file in its place.
 */
function DropOverTarget({ name }: { name: string }) {
  return (
    <div
      aria-hidden
      className='pointer-events-none absolute bottom-[180px] left-0 right-[340px] top-0 m-4 grid place-items-center rounded-xl border-[1.5px] border-dashed border-accent bg-stage/80'
    >
      <p className='text-lg font-medium'>Drop to open it in place of {name}</p>
    </div>
  );
}

/** The inspector's Output tab with nothing open: what it will hold, and what carries over. */
function NothingOpen({ settings }: { settings: OutputSettings }) {
  return (
    <div className='flex h-full flex-col items-center justify-center gap-2 px-10 text-center text-dim'>
      <svg
        aria-hidden
        width='28'
        height='28'
        viewBox='0 0 24 24'
        fill='none'
        stroke='#55595f'
        strokeWidth='1.5'
      >
        <path d='M4 6h16M4 12h10M4 18h13' />
      </svg>
      <p>Output settings appear here once a video is open.</p>
      <p className='text-sm text-faint'>
        Carried over: {carriedOverSummary(settings)}
      </p>
    </div>
  );
}
