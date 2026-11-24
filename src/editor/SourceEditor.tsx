import { useRef, useState } from 'react';

import ExportFooter, { type SaveTo } from './ExportFooter';
import Inspector from './Inspector';
import OutputPreview from './OutputPreview';
import OutputSettingsForm from './OutputSettingsForm';
import type { MemoryEstimate } from '../ipc/export';
import { hasAudio } from '../ipc/probe';
import type { ExportJob, ExportNotice } from './useExport';
import { usePlaying, useSizeEstimate } from './useSizeEstimate';
import { fileName, type OpenSource } from './useSource';
import { useClipShortcuts } from './useClipShortcuts';
import VideoTrimmer from './VideoTrimmer';
import {
  PaneLabel,
  PlaceholderColumn,
  PreviewCopyProgress,
  ViewerHeader,
  fitAspect,
  useViewerMode,
} from './Viewer';
import {
  OutputSettings,
  croppedSourceSize,
  fileTypeOrder,
  formatSpeed,
  frameCount,
  isSideways,
  outputSeconds,
  outputSize,
} from './lib/output-settings';
import { hasCaptions, renderCaptionImage } from './lib/captions';
import type { Size } from './lib/output-preview';
import {
  ClipCrop,
  ClipTrim,
  ExportRequest,
  SessionResult,
} from './lib/types';

/**
 * The editor with a source open: the viewer and timeline on the left, the inspector on the
 * right, and one export run per ticked file type with the user's own settings.
 *
 * Sizes, length and sound come from the probe's source info rather than the
 * player, which may be showing a proxy at a lower resolution: every pixel
 * figure is in the source's displayed pixels, which is what export crops.
 */
export default function SourceEditor({
  source,
  trim,
  crop,
  settings,
  results,
  memory,
  onSettingsChange,
  onTrimChange,
  onCropChange,
  onUnplayable,
  job,
  saveTo,
  onExport,
  onCancel,
}: {
  source: OpenSource;
  trim?: ClipTrim;
  crop?: ClipCrop;
  settings: OutputSettings;
  results: readonly SessionResult[];
  /** The frame-memory estimate for this selection, crop and settings. */
  memory: MemoryEstimate | null;
  onSettingsChange: (settings: OutputSettings) => void;
  onTrimChange: (trim: ClipTrim | undefined) => void;
  onCropChange: (crop: ClipCrop | undefined) => void;
  onUnplayable: () => void;
  /** The running export, which may have started before this clip was opened. */
  job: ExportJob | null;
  saveTo: SaveTo;
  /** Exports into `saveTo`'s folder; what it resolves with is shown under the button. */
  onExport: (request: ExportRequest, destination: string) => Promise<ExportNotice>;
  onCancel: () => void;
}) {
  const editor = useRef<HTMLDivElement>(null);
  const [mode, setMode] = useViewerMode();
  const [media, setMedia] = useState<{
    src: string;
    video: HTMLVideoElement;
  } | null>(null);
  // Covers drawing the caption image, before the export (and so `job`) starts.
  const [preparing, setPreparing] = useState(false);
  const [notice, setNotice] = useState<ExportNotice | null>(null);
  const converting = preparing || job !== null;
  const { destination } = saveTo;

  const { path, info, preview } = source;
  const src = preview.stage === 'ready' ? preview.src : null;
  // The element belongs to the preview source it played; a proxy replacing
  // the source starts without one.
  const video = media && media.src === src ? media.video : null;
  const cropped = croppedSourceSize(crop, info);
  const size = outputSize(settings.width, cropped, settings.rotation);
  const selectionSeconds = trim ? trim.end - trim.start : info.duration;
  const seconds = outputSeconds(selectionSeconds, settings);
  const frames = frameCount(seconds, settings.fps);
  const playing = usePlaying(video);
  // Estimates are background work: they wait while the video plays or an export runs.
  const estimates = useSizeEstimate(
    { path, info, trim, crop, settings, size },
    playing || converting
  );
  const paging = memory !== null && !memory.fits;
  const canExport =
    !converting && settings.fileTypes.length > 0 && destination !== null;

  const startExport = async () => {
    if (!canExport || !destination) return;
    const fileTypes = fileTypeOrder.filter((type) =>
      settings.fileTypes.includes(type)
    );
    setNotice(null);
    setPreparing(true);
    try {
      const captions = hasCaptions(settings)
        ? ((await renderCaptionImage(settings, size)) ?? undefined)
        : undefined;
      const exporting = onExport(
        {
          path,
          info,
          trim,
          crop,
          settings: { ...settings, fileTypes },
          keepSound: settings.keepSound && hasAudio(info),
          captions,
          acceptPaging: paging,
        },
        destination.folder
      );
      setPreparing(false);
      setNotice(await exporting);
    } catch (error) {
      setNotice({
        text: 'Exporting failed.',
        details: error instanceof Error ? error.message : String(error),
        failed: true,
      });
    } finally {
      setPreparing(false);
    }
  };

  useClipShortcuts(
    { export: () => void startExport() },
    { enabled: canExport, scope: editor }
  );

  const outputPane = (area: Size) => {
    const shown = fitAspect(area, size.width / size.height);
    return (
      <figure
        aria-label='Output preview'
        className='flex min-w-0 flex-col gap-1.5'
        style={{ width: shown.width }}
      >
        <figcaption>
          <PaneLabel>
            Output · {size.width} × {size.height} · {settings.fps} fps · plays
            at {formatSpeed(settings.speed)}
          </PaneLabel>
        </figcaption>
        <div className='relative bg-black shadow-[0_0_0_1px_rgb(var(--line-control))]'>
          <OutputPreview
            video={video}
            crop={crop}
            // The drawing's longer side; it keeps the output's aspect inside this square.
            box={Math.max(1, Math.floor(Math.max(shown.width, shown.height)))}
            fit='contain'
            outputWidth={size.width}
            orientation={settings}
            captions={{ captions: settings, output: size }}
            label='Output preview drawing'
            className='block'
          />
          {settings.direction !== 'forward' && (
            <p className='absolute bottom-2 left-2 right-2 w-fit rounded bg-black/70 px-1.5 py-0.5 text-xs text-white'>
              The preview plays forward.{' '}
              {settings.direction === 'reverse' ? 'Reverse' : 'Boomerang'} is
              applied when you export.
            </p>
          )}
        </div>
      </figure>
    );
  };

  return (
    <div
      ref={editor}
      className='grid min-h-0 grid-cols-[minmax(0,1fr)_340px]'
    >
      {preview.stage === 'building' ? (
        <PlaceholderColumn
          header={<ViewerHeader mode={mode} onModeChange={setMode} />}
        >
          <PreviewCopyProgress
            name={fileName(path)}
            progress={preview.progress}
          />
        </PlaceholderColumn>
      ) : (
        <VideoTrimmer
          // A proxy replacing the source starts the player afresh.
          key={preview.src}
          source={{ ...info, path, src: preview.src }}
          trim={trim}
          crop={crop}
          disabled={converting}
          card={editor}
          mode={mode}
          onModeChange={setMode}
          output={outputPane}
          playbackRate={settings.speed}
          sideways={isSideways(settings.rotation)}
          onChange={onTrimChange}
          onCropChange={onCropChange}
          onMediaLoaded={(element) =>
            setMedia({ src: preview.src, video: element })
          }
          onUnplayable={onUnplayable}
        />
      )}
      <Inspector
        results={results}
        output={
          // A fieldset defaults to min-width: min-content, so without min-w-0
          // wide settings rows push past the column instead of shrinking.
          <fieldset disabled={converting} className='min-w-0 disabled:opacity-60'>
            <legend className='sr-only'>Output settings</legend>
            <OutputSettingsForm
              settings={settings}
              onChange={onSettingsChange}
              outputWidth={size.width}
              outputHeight={size.height}
              nativeWidth={size.native}
              hasAudio={hasAudio(info)}
            />
          </fieldset>
        }
        footer={
          <ExportFooter
            settings={settings}
            seconds={seconds}
            frames={frames}
            estimates={estimates}
            playing={playing}
            memory={memory}
            saveTo={saveTo}
            converting={converting}
            job={job}
            notice={notice}
            onExport={() => void startExport()}
            onCancel={onCancel}
          />
        }
      />
    </div>
  );
}
