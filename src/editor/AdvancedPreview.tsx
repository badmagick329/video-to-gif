
import { ClipCrop } from './lib/types';
import { Size } from './lib/output-preview';
import {
  AdvancedSettings,
  FileType,
  fileTypeInfo,
  fileTypeOrder,
  formatBytes,
  formatSpeed,
} from './lib/advanced';
import OutputPreview from './OutputPreview';
import type { FileTypeEstimate } from './useSizeEstimate';

/** Fits the 390 px phone's settings column, the narrowest it gets. */
const previewBox = 300;

const pill =
  'rounded-full border border-fg/20 px-2 py-0.5 text-xs tabular-nums';

/**
 * Gif Studio's *Output preview*: the selection as it will encode, with the
 * file's exact size, length and frame count. It draws rotation, flips and
 * captions, and moves at the chosen speed because the editor's video plays
 * at it.
 * Reverse and Boomerang are labelled rather than played, as in Gif Studio:
 * playing them would need every frame of the selection decoded and held.
 */
export default function AdvancedPreview({
  video,
  crop,
  settings,
  size,
  seconds,
  frames,
  estimates,
  playing,
}: {
  video: HTMLVideoElement | null;
  crop?: ClipCrop;
  settings: Pick<
    AdvancedSettings,
    | 'rotation'
    | 'flipHorizontal'
    | 'flipVertical'
    | 'speed'
    | 'direction'
    | 'topText'
    | 'bottomText'
    | 'captionSize'
  >;
  size: Size | null;
  seconds: number | null;
  frames: number | null;
  /** Each ticked file type's size estimate. */
  estimates: Partial<Record<FileType, FileTypeEstimate>>;
  /** Estimates wait while the video plays. */
  playing: boolean;
}) {
  return (
    <figure
      aria-label='Output preview'
      className='flex flex-col gap-2 rounded-md bg-canvas/60 p-3'
    >
      <figcaption className='flex items-center justify-between text-sm font-semibold'>
        Output preview
        <span className='text-xs font-normal text-fg/60'>
          live{settings.speed !== 1 && ` · ${formatSpeed(settings.speed)}`}
        </span>
      </figcaption>
      <div
        className='grid place-items-center rounded bg-black/60'
        style={{ minHeight: previewBox / 2 }}
      >
        {size && (
          <OutputPreview
            video={video}
            crop={crop}
            box={previewBox}
            fit='contain'
            outputWidth={size.width}
            orientation={settings}
            captions={{ captions: settings, output: size }}
            label='Output preview drawing'
          />
        )}
      </div>
      {settings.direction !== 'forward' && (
        <p className='text-xs text-fg/70'>
          The preview plays forward.{' '}
          {settings.direction === 'reverse' ? 'Reverse' : 'Boomerang'} is
          applied when you convert.
        </p>
      )}
      {size && (
        <div className='flex flex-wrap gap-1.5' aria-label='Output details'>
          <span className={pill}>
            <b>
              {size.width} × {size.height}
            </b>
          </span>
          {seconds !== null && (
            <span className={pill}>
              <b>{seconds.toFixed(1)} s</b>
            </span>
          )}
          {frames !== null && <span className={pill}>{frames} frames</span>}
          {fileTypeOrder.map((type) => {
            const estimate = estimates[type];
            return (
              estimate && (
                <SizePill key={type} fileType={type} estimate={estimate} />
              )
            );
          })}
        </div>
      )}
      {size && <EstimateNote estimates={estimates} playing={playing} />}
    </figure>
  );
}

/**
 * One file type's expected size. The figure is the whole message: no limit or judgement. While
 * a newer estimate runs, the old figure stays, greyed, rather than blanking.
 */
function SizePill({
  fileType,
  estimate,
}: {
  fileType: FileType;
  estimate: FileTypeEstimate;
}) {
  const label = fileTypeInfo[fileType].label;
  switch (estimate.state) {
    case 'estimating':
      return (
        <span className={pill}>
          {label}{' '}
          <span
            role='img'
            aria-label='Estimating'
            className='inline-block h-2.5 w-12 animate-pulse rounded-sm bg-fg/20 align-middle'
          />
        </span>
      );
    case 'failed':
      return (
        <span className={pill} title={estimate.reason}>
          {label} size unknown
        </span>
      );
    case 'ready':
    case 'stale':
      return (
        <span
          className={pill}
          aria-label={`${label} about ${formatBytes(estimate.bytes)}${estimate.state === 'stale' ? ', updating' : ''}`}
        >
          {label}{' '}
          <b className={estimate.state === 'stale' ? 'text-fg/40' : undefined}>
            ≈ {formatBytes(estimate.bytes)}
          </b>
        </span>
      );
  }
}

/** What the size figures are and whether they are up to date. */
function EstimateNote({
  estimates,
  playing,
}: {
  estimates: Partial<Record<FileType, FileTypeEstimate>>;
  playing: boolean;
}) {
  const states = Object.values(estimates).map((estimate) => estimate.state);
  if (states.length === 0) return null;
  const pending = states.some(
    (state) => state === 'estimating' || state === 'stale'
  );
  let text = 'Sizes come from samples of this selection, so expect some drift.';
  if (pending && playing) text = 'Sizes update when the video stops.';
  else if (states.includes('stale')) text = 'Updating sizes…';
  else if (pending) text = 'Encoding short samples of the selection…';
  return <p className='text-xs text-fg/60'>{text}</p>;
}
