import { useRef, useState } from 'react';
import { IoIosCloseCircle } from 'react-icons/io';

import AdvancedPreview from './AdvancedPreview';
import AdvancedSettingsForm from './AdvancedSettingsForm';
import SessionResults, { ErrorDetails } from './SessionResults';
import type { MemoryEstimate } from '../ipc/export';
import { hasAudio } from '../ipc/probe';
import type { Destination } from './useDestination';
import type { ExportJob, ExportNotice } from './useExport';
import { useMemoryEstimate } from './useMemoryEstimate';
import { fileName, type OpenSource } from './useSource';
import { useClipShortcuts } from './useClipShortcuts';
import VideoTrimmer from './VideoTrimmer';
import {
  AdvancedSettings,
  croppedSourceSize,
  fileTypeInfo,
  fileTypeOrder,
  formatBytes,
  frameCount,
  isLargeGif,
  isSideways,
  outputSeconds,
  outputSize,
} from './lib/advanced';
import { hasCaptions, renderCaptionImage } from './lib/captions';
import { formatClipTime } from './lib/trim';
import {
  ClipCrop,
  ClipTrim,
  ExportRequest,
  SessionResult,
} from './lib/types';

/** The Save to row's state and actions, from `useDestination`. */
export type SaveTo = {
  destination: Destination | null;
  error: string;
  choose: () => Promise<void>;
  saveBesideSource: () => Promise<void>;
};

/** mgck's `truncateText`, from its shared utils. */
function truncateText(text: string, max: number = 28) {
  return text.length > max - 3 ? `${text.slice(0, max)}...` : text;
}

/**
 * Advanced mode: one clip, Gif Studio's two-column editor, and a single
 * export run per ticked file type with the user's own settings.
 *
 * Sizes, length and sound come from the probe's source info rather than the
 * player, which may be showing a proxy at a lower resolution: every pixel
 * figure is in the source's displayed pixels, which is what export crops.
 */
export default function AdvancedEditor({
  source,
  trim,
  crop,
  settings,
  results,
  onSettingsChange,
  onTrimChange,
  onCropChange,
  onRemove,
  onUnplayable,
  job,
  saveTo,
  onExport,
  onCancel,
}: {
  source: OpenSource;
  trim?: ClipTrim;
  crop?: ClipCrop;
  settings: AdvancedSettings;
  results: readonly SessionResult[];
  onSettingsChange: (settings: AdvancedSettings) => void;
  onTrimChange: (trim: ClipTrim | undefined) => void;
  onCropChange: (crop: ClipCrop | undefined) => void;
  onRemove: () => void;
  onUnplayable: () => void;
  /** The running export, which may have started before this clip was opened. */
  job: ExportJob | null;
  saveTo: SaveTo;
  /** Exports into `saveTo`'s folder; what it resolves with is shown under the button. */
  onExport: (request: ExportRequest, destination: string) => Promise<ExportNotice>;
  onCancel: () => void;
}) {
  const editor = useRef<HTMLDivElement>(null);
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
  const name = fileName(path);
  const src = preview.stage === 'ready' ? preview.src : null;
  // The element belongs to the preview source it played; a proxy replacing
  // the source starts without one.
  const video = media && media.src === src ? media.video : null;
  const cropped = croppedSourceSize(crop, info);
  const size = outputSize(settings.width, cropped, settings.rotation);
  const selectionSeconds = trim ? trim.end - trim.start : info.duration;
  const seconds = outputSeconds(selectionSeconds, settings);
  const frames = frameCount(seconds, settings.fps);
  const largeGif =
    settings.fileTypes.includes('gif') && isLargeGif(size, frames);
  const memory = useMemoryEstimate({ info, trim, crop, settings });
  // The engine refuses a run whose held frames exceed free memory; say so before Convert.
  const tooBig = memory !== null && !memory.fits;
  const canConvert =
    !converting &&
    settings.fileTypes.length > 0 &&
    !tooBig &&
    destination !== null;

  const convert = async () => {
    if (!canConvert || !destination) return;
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
        },
        destination.folder
      );
      setPreparing(false);
      setNotice(await exporting);
    } catch (error) {
      setNotice({
        text: 'Converting failed.',
        details: error instanceof Error ? error.message : String(error),
        failed: true,
      });
    } finally {
      setPreparing(false);
    }
  };

  useClipShortcuts(
    { convert: () => void convert() },
    { enabled: canConvert, scope: editor }
  );

  return (
    <div
      ref={editor}
      className='grid w-full max-w-6xl grid-cols-1 gap-5 lg:grid-cols-[minmax(0,1fr)_23rem] lg:items-start'
    >
      <div className='flex min-w-0 flex-col gap-3 rounded-md bg-secondary-dg p-3 shadow-glow-secondary-dg'>
        <div className='flex items-center justify-between gap-2'>
          <p className='truncate' title={path}>
            {truncateText(name, 40)}
            {preview.stage === 'ready' && preview.kind === 'proxy' && (
              <span className='ml-2 text-xs text-foreground-dg/60'>
                (preview copy)
              </span>
            )}
          </p>
          <button
            type='button'
            className='text-foreground-dg/80 hover:text-foreground-dg disabled:opacity-50'
            disabled={converting}
            aria-label={`Remove ${name}`}
            onClick={onRemove}
          >
            <IoIosCloseCircle />
          </button>
        </div>
        {preview.stage === 'building' ? (
          <ProxyProgress name={name} progress={preview.progress} />
        ) : (
          <VideoTrimmer
            // A proxy replacing the source starts the player afresh.
            key={preview.src}
            source={{ ...info, path, src: preview.src }}
            trim={trim}
            crop={crop}
            disabled={converting}
            card={editor}
            videoMaxHeight={420}
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
      </div>
      <aside
        aria-label='Output settings'
        className='flex min-w-0 flex-col gap-4 rounded-md bg-secondary-dg p-3 text-left shadow-glow-secondary-dg'
      >
        <AdvancedPreview
          video={video}
          crop={crop}
          settings={settings}
          size={size}
          seconds={seconds}
          frames={frames}
          largeGif={largeGif}
        />
        {/* A fieldset defaults to min-width: min-content, so without min-w-0
            wide settings rows push past the column instead of shrinking. */}
        <fieldset disabled={converting} className='min-w-0 disabled:opacity-60'>
          <legend className='sr-only'>Output settings</legend>
          <AdvancedSettingsForm
            settings={settings}
            onChange={onSettingsChange}
            outputWidth={size.width}
            nativeWidth={size.native}
            hasAudio={hasAudio(info)}
          />
        </fieldset>
        <SaveToRow saveTo={saveTo} disabled={converting} />
        <div className='flex flex-col gap-2'>
          {job ? (
            <>
              <ExportProgress job={job} />
              <button
                type='button'
                className='h-9 rounded-md border border-foreground-dg/25 text-sm hover:bg-foreground-dg/10 disabled:opacity-50'
                disabled={job.cancelling}
                onClick={onCancel}
              >
                {job.cancelling ? 'Cancelling…' : 'Cancel'}
              </button>
            </>
          ) : (
            <button
              type='button'
              className='rounded-md bg-primary-dg px-4 py-3 font-semibold hover:bg-primary-dg/80 disabled:cursor-not-allowed disabled:opacity-50'
              disabled={!canConvert}
              onClick={() => void convert()}
            >
              {settings.fileTypes.length === 0
                ? 'Pick a file type to save as'
                : `Convert to ${fileTypeOrder
                    .filter((type) => settings.fileTypes.includes(type))
                    .map((type) => fileTypeInfo[type].label)
                    .join(' + ')}`}
            </button>
          )}
          {memory && <MemoryWarning estimate={memory} settings={settings} />}
          {notice && (
            <div
              role={notice.failed ? 'alert' : 'status'}
              className='flex flex-col gap-1 text-sm text-foreground-dg/80'
            >
              <p className={notice.failed ? 'text-red-500' : undefined}>
                {notice.text}
              </p>
              {notice.details && (
                <ErrorDetails summary='Details' details={notice.details} />
              )}
            </div>
          )}
        </div>
        <SessionResults results={results} />
      </aside>
    </div>
  );
}

/** mgck's conversion progress bar: the file type being made, which of how many, and how far. */
function ExportProgress({ job }: { job: ExportJob }) {
  const index = Math.max(0, job.fileTypes.indexOf(job.fileType));
  const progress = (index + job.done) / job.fileTypes.length;
  return (
    <div
      role='progressbar'
      aria-label='Conversion progress'
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(progress * 100)}
      className='relative h-11 overflow-hidden rounded-md bg-background-dg'
    >
      <div
        className='absolute inset-y-0 left-0 bg-primary-dg/70 transition-[width]'
        style={{ width: `${progress * 100}%` }}
      />
      <p className='relative grid h-full place-items-center text-sm font-semibold'>
        {progressText(job, index)}
      </p>
    </div>
  );
}

function progressText(job: ExportJob, index: number): string {
  const label = fileTypeInfo[job.fileType].label;
  const total = job.fileTypes.length;
  const count = total > 1 ? ` (${index + 1}/${total})` : '';
  // A GIF's palette pass reads every frame before the first is written.
  if (job.done === 0)
    return `Making ${label}${count}${job.fileType === 'gif' ? ', analysing colours' : ''}…`;
  return `Making ${label}${count} ${Math.round(job.done * 100)}%`;
}

/** Where Convert saves, with Save to's folder picker. */
function SaveToRow({ saveTo, disabled }: { saveTo: SaveTo; disabled: boolean }) {
  const { destination, error, choose, saveBesideSource } = saveTo;
  const button =
    'rounded-md border border-foreground-dg/25 px-2 py-1 text-xs hover:bg-foreground-dg/10 disabled:opacity-50';
  return (
    <div
      role='group'
      aria-label='Save to'
      className='flex flex-col gap-1.5 text-sm'
    >
      <p className='min-w-0'>
        <span className='text-foreground-dg/70'>Save to </span>
        <span className='break-all font-semibold' title={destination?.folder}>
          {destination?.folder ?? '…'}
        </span>
        {destination?.from === 'source' && (
          <span className='text-foreground-dg/60'> (the video&apos;s folder)</span>
        )}
      </p>
      <div className='flex flex-wrap gap-2'>
        <button
          type='button'
          className={button}
          disabled={disabled}
          onClick={() => void choose()}
        >
          Choose folder…
        </button>
        {destination?.from === 'chosen' && (
          <button
            type='button'
            className={button}
            disabled={disabled}
            onClick={() => void saveBesideSource()}
          >
            Use the video&apos;s folder
          </button>
        )}
      </div>
      {error && (
        <p role='alert' className='text-xs text-red-500'>
          {error}
        </p>
      )}
    </div>
  );
}

/**
 * The frame-memory guard's estimate, worded for this machine's free memory. Replaces mgck's
 * phone and desktop budgets: the budget here is what Windows reports free.
 */
function MemoryWarning({
  estimate,
  settings,
}: {
  estimate: MemoryEstimate;
  settings: AdvancedSettings;
}) {
  const { heldBytes, availableBytes, limitSeconds, fits } = estimate;
  if (heldBytes === 0 || availableBytes === null) return null;
  const holder = memoryHolder(settings.direction);
  const held = formatBytes(heldBytes);
  const free = formatBytes(availableBytes);
  if (!fits)
    return (
      <p role='alert' className='text-sm text-amber-200'>
        {holder} keeps every frame in memory: about {held}, and {free} is free
        now, so this selection is too long to convert.{' '}
        {limitSeconds !== null && limitSeconds > 0
          ? `Trim it to ${formatClipTime(Math.floor(limitSeconds * 100) / 100)} or less, or lower the width or frame rate.`
          : 'Lower the width or frame rate.'}
      </p>
    );
  // Fits, but a run this big can push the rest of the PC into its page file.
  if (heldBytes > availableBytes / 2)
    return (
      <p className='text-sm text-amber-200'>
        {holder} will keep about {held} of frames in memory, more than half of
        the {free} free now.
      </p>
    );
  return null;
}

/** What holds the frames: the direction, or else the GIF's palette pass. */
function memoryHolder(direction: AdvancedSettings['direction']): string {
  if (direction === 'reverse') return 'Reverse';
  if (direction === 'boomerang') return 'Boomerang';
  return 'Making a GIF';
}

/** Shown in the player's place while ffmpeg makes the proxy. */
function ProxyProgress({ name, progress }: { name: string; progress: number }) {
  const percent = Math.round(progress * 100);
  return (
    <div className='flex flex-col gap-2 py-6'>
      <p>
        Making a preview copy of {name}: {percent}%
      </p>
      <div
        role='progressbar'
        aria-label='Preview copy progress'
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent}
        className='relative h-2 overflow-hidden rounded-md bg-background-dg'
      >
        <div
          className='absolute inset-y-0 left-0 bg-primary-dg/70 transition-[width]'
          style={{ width: `${percent}%` }}
        />
      </div>
      <p className='text-xs text-foreground-dg/70'>
        The player can&apos;t show this file as it is, so ffmpeg makes a
        smaller copy to trim and crop against. Your files are still made from
        the original.
      </p>
    </div>
  );
}
