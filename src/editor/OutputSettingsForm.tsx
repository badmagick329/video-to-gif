import clsx from 'clsx';
import { ReactNode, useId, useState } from 'react';
import { LuChevronRight, LuRotateCcw, LuRotateCw } from 'react-icons/lu';
import { Switch } from './Switch';
import { Segmented } from './Viewer';
import {
  AdvancedSettings,
  FileType,
  clampSetting,
  directionOptions,
  fileTypeInfo,
  fileTypeOrder,
  formatSpeed,
  gifDitherOptions,
  playsOptions,
  rotate,
  settingLimits,
  soundDropReason,
  speedSteps,
  widthChoices,
} from './lib/advanced';

const mini =
  'h-6 rounded-[5px] border border-line-control px-2 text-sm text-fg-soft hover:text-fg aria-pressed:border-accent aria-pressed:bg-accent/15 aria-pressed:text-white disabled:cursor-not-allowed disabled:opacity-40';
const field =
  'h-[26px] rounded-[5px] border border-line-control bg-canvas px-1.5 text-sm text-fg focus:border-accent focus:outline-none';
const text =
  'h-7 w-full rounded-[5px] border border-line-control bg-canvas px-2 text-md text-fg placeholder:text-[#7f848e] focus:border-accent focus:outline-none';
const readout =
  'flex h-6 w-[58px] items-center justify-end rounded-[5px] border border-line-control bg-canvas px-1.5 font-mono text-sm tabular-nums';
const button =
  'inline-flex h-7 items-center gap-1.5 rounded-md border border-line-strong bg-control px-2.5 text-md hover:bg-control-hover aria-pressed:border-accent aria-pressed:bg-accent/15';

/** Short notes under each file type's toggle; the toggle is 100 px wide. */
const fileTypeNotes: Record<FileType, string> = {
  gif: 'main output',
  webp: 'animated',
  mp4: 'H.264',
};

/** The dithering option's name without its explanation, for the section summary. */
const shortLabel = (label: string) => label.replace(/\s*\(.*\)$/, '');

/**
 * The inspector's Output tab: Format, Size, Timing, Transform, Captions, and the folded GIF
 * quality and looping settings. Each setting is the user's explicit choice; nothing targets a size.
 */
export default function OutputSettingsForm({
  settings,
  onChange,
  outputWidth,
  outputHeight,
  nativeWidth,
  hasAudio,
}: {
  settings: AdvancedSettings;
  onChange: (settings: AdvancedSettings) => void;
  /** The width the file will have, after capping. */
  outputWidth: number | null;
  outputHeight: number | null;
  /** The crop's even width, the most the output can have. */
  nativeWidth: number | null;
  /** From the probe, so it holds for the source whether the player shows it or a proxy. */
  hasAudio: boolean;
}) {
  const update = (patch: Partial<AdvancedSettings>) =>
    onChange({ ...settings, ...patch });
  const [qualityOpen, setQualityOpen] = useState(false);
  const fpsId = useId();
  const colorsId = useId();
  const ditherId = useId();
  const playsId = useId();
  const webpId = useId();
  const encoderId = useId();
  const soundId = useId();
  const soundNoteId = useId();
  const widthId = useId();
  const speedId = useId();
  const captionSizeId = useId();
  const qualityId = useId();
  const speedIndex = Math.max(
    0,
    speedSteps.findIndex((step) => step === settings.speed)
  );
  const soundDropped = soundDropReason(settings);
  const shownWidth =
    outputWidth ?? (settings.width === 'original' ? null : settings.width);
  const sliderMax = nativeWidth ?? 1280;
  const tooWide = (width: number) => nativeWidth !== null && width > nativeWidth;
  const ticked = (fileType: FileType) => settings.fileTypes.includes(fileType);
  const dither = gifDitherOptions.find(
    (option) => option.value === settings.gifDither
  );
  const plays = playsOptions.find((option) => option.value === settings.plays);
  const qualitySummary = [
    `${settings.gifColors} colours`,
    dither && shortLabel(dither.label),
    plays?.label,
  ]
    .filter(Boolean)
    .join(' · ');

  const toggleFileType = (fileType: FileType) =>
    update({
      fileTypes: fileTypeOrder.filter((type) =>
        type === fileType ? !ticked(type) : ticked(type)
      ),
    });

  return (
    <div className='flex flex-col'>
      <Section title='Format'>
        <div role='group' aria-label='Formats' className='grid grid-cols-3 gap-1.5'>
          {fileTypeOrder.map((fileType) => (
            <button
              key={fileType}
              type='button'
              aria-pressed={ticked(fileType)}
              aria-label={fileTypeInfo[fileType].label}
              className='flex h-[46px] flex-col justify-center gap-px rounded-[7px] border border-line-control bg-well px-2.5 text-left hover:border-line-strong aria-pressed:border-accent aria-pressed:bg-accent/[.12]'
              onClick={() => toggleFileType(fileType)}
            >
              <b className='text-base font-semibold text-fg'>
                {fileTypeInfo[fileType].label}
              </b>
              <small className='text-xs text-dim'>{fileTypeNotes[fileType]}</small>
            </button>
          ))}
        </div>
        <Row label='GIF encoder' htmlFor={encoderId} wide>
          {/* gifski joins this list in its own Task (VTG-15). */}
          <select id={encoderId} className={field} defaultValue='ffmpeg'>
            <option value='ffmpeg'>FFmpeg (palette)</option>
          </select>
        </Row>
        {ticked('webp') && (
          <Row label='WebP quality' htmlFor={webpId}>
            <input
              id={webpId}
              type='range'
              className='w-full'
              min={settingLimits.webpQuality.min}
              max={settingLimits.webpQuality.max}
              step={1}
              value={settings.webpQuality}
              onChange={(event) =>
                update({
                  webpQuality: clampSetting(
                    'webpQuality',
                    Number(event.target.value)
                  ),
                })
              }
            />
            <span className={readout}>{settings.webpQuality}</span>
          </Row>
        )}
        {/* A source without sound has nothing to keep. */}
        {ticked('mp4') && hasAudio && (
          <div className='flex flex-col gap-1'>
            <span className='flex items-center gap-2 text-md text-fg-label'>
              <Switch
                id={soundId}
                checked={settings.keepSound}
                onCheckedChange={(keepSound) => update({ keepSound })}
                aria-describedby={
                  settings.keepSound && soundDropped ? soundNoteId : undefined
                }
              />
              <label htmlFor={soundId} className='cursor-pointer'>
                Keep sound in MP4
              </label>
            </span>
            {settings.keepSound && soundDropped && (
              <p id={soundNoteId} className='text-sm text-warn'>
                {soundDropped}
              </p>
            )}
          </div>
        )}
      </Section>

      <Section title='Size'>
        <Row label='Width' htmlFor={widthId}>
          <input
            id={widthId}
            type='range'
            className='w-full'
            min={Math.min(120, sliderMax)}
            max={sliderMax}
            step={2}
            value={shownWidth ?? sliderMax}
            onChange={(event) => update({ width: Number(event.target.value) })}
          />
          <WidthField
            value={shownWidth}
            max={nativeWidth ?? settingLimits.width.max}
            onCommit={(width) => update({ width })}
          />
        </Row>
        <div className='flex flex-wrap gap-1' role='group' aria-label='Width presets'>
          {widthChoices.map((width) => (
            <button
              key={width}
              type='button'
              className={mini}
              // Output never exceeds the crop, so a wider preset would only
              // look selected while producing the crop's width.
              aria-pressed={settings.width === width && !tooWide(width)}
              disabled={tooWide(width)}
              onClick={() => update({ width })}
            >
              {width}
            </button>
          ))}
          <button
            type='button'
            className={mini}
            aria-pressed={settings.width === 'original'}
            onClick={() => update({ width: 'original' })}
          >
            Original
          </button>
        </div>
        {outputWidth !== null && outputHeight !== null && (
          <p aria-label='Output size' className='text-sm text-muted'>
            Output{' '}
            <b className='font-mono text-[11.5px] font-medium text-fg'>
              {outputWidth} × {outputHeight}
            </b>
          </p>
        )}
      </Section>

      <Section title='Timing'>
        <Row label='Frame rate' htmlFor={fpsId}>
          <input
            id={fpsId}
            type='range'
            className='w-full'
            min={settingLimits.fps.min}
            max={settingLimits.fps.max}
            step={1}
            value={settings.fps}
            onChange={(event) =>
              update({ fps: clampSetting('fps', Number(event.target.value)) })
            }
          />
          <span className={readout}>{settings.fps} fps</span>
        </Row>
        <Row label='Speed' htmlFor={speedId}>
          <input
            id={speedId}
            type='range'
            className='w-full'
            min={0}
            max={speedSteps.length - 1}
            step={1}
            value={speedIndex}
            aria-valuetext={formatSpeed(settings.speed)}
            onChange={(event) =>
              update({ speed: speedSteps[Number(event.target.value)] })
            }
          />
          <span className={readout}>{formatSpeed(settings.speed)}</span>
        </Row>
        <Segmented
          label='Direction'
          options={directionOptions}
          value={settings.direction}
          onChange={(direction) => update({ direction })}
        />
      </Section>

      <Section title='Transform'>
        <div className='flex flex-wrap items-center gap-1'>
          <button
            type='button'
            className={button}
            aria-label='Rotate left'
            title='Rotate left'
            onClick={() => update({ rotation: rotate(settings.rotation, 'left') })}
          >
            <LuRotateCcw aria-hidden className='h-[15px] w-[15px]' />
          </button>
          <button
            type='button'
            className={button}
            aria-label='Rotate right'
            title='Rotate right'
            onClick={() =>
              update({ rotation: rotate(settings.rotation, 'right') })
            }
          >
            <LuRotateCw aria-hidden className='h-[15px] w-[15px]' />
          </button>
          <button
            type='button'
            className={button}
            aria-pressed={settings.flipHorizontal}
            title='Mirror left to right'
            onClick={() => update({ flipHorizontal: !settings.flipHorizontal })}
          >
            Flip ↔
          </button>
          <button
            type='button'
            className={button}
            aria-pressed={settings.flipVertical}
            title='Flip upside down'
            onClick={() => update({ flipVertical: !settings.flipVertical })}
          >
            Flip ↕
          </button>
          {settings.rotation !== 0 && (
            <span className='ml-1 text-sm tabular-nums text-muted'>
              Turned {settings.rotation}°
            </span>
          )}
        </div>
      </Section>

      <Section title='Captions'>
        <input
          type='text'
          aria-label='Top text'
          className={text}
          placeholder='Top caption'
          value={settings.topText}
          onChange={(event) => update({ topText: event.target.value })}
        />
        <input
          type='text'
          aria-label='Bottom text'
          className={text}
          placeholder='Bottom caption'
          value={settings.bottomText}
          onChange={(event) => update({ bottomText: event.target.value })}
        />
        <Row label='Text size' htmlFor={captionSizeId}>
          <input
            id={captionSizeId}
            type='range'
            className='w-full'
            min={settingLimits.captionSize.min}
            max={settingLimits.captionSize.max}
            step={1}
            value={settings.captionSize}
            aria-valuetext={`${settings.captionSize}% of the shorter side`}
            onChange={(event) =>
              update({
                captionSize: clampSetting(
                  'captionSize',
                  Number(event.target.value)
                ),
              })
            }
          />
          <span className={readout}>{settings.captionSize}%</span>
        </Row>
      </Section>

      <section className='flex flex-col gap-[9px] border-b border-line-subtle px-3.5 pb-3 pt-2.5'>
        <h3 className='flex items-center justify-between gap-2 text-xs font-semibold uppercase tracking-[.07em] text-dim'>
          <button
            type='button'
            aria-expanded={qualityOpen}
            aria-controls={qualityId}
            className='flex shrink-0 items-center gap-1.5 whitespace-nowrap uppercase tracking-[.07em]'
            onClick={() => setQualityOpen((open) => !open)}
          >
            <LuChevronRight
              aria-hidden
              className={clsx(
                'h-2.5 w-2.5 stroke-[3] transition-transform',
                qualityOpen && 'rotate-90'
              )}
            />
            GIF quality &amp; looping
          </button>
          <span className='min-w-0 truncate font-normal normal-case tracking-normal'>
            {qualitySummary}
          </span>
        </h3>
        {qualityOpen && (
          <div id={qualityId} className='flex flex-col gap-[9px]'>
            <Row label='Colours' htmlFor={colorsId}>
              <input
                id={colorsId}
                type='range'
                className='w-full'
                min={settingLimits.gifColors.min}
                max={settingLimits.gifColors.max}
                step={1}
                value={settings.gifColors}
                onChange={(event) =>
                  update({
                    gifColors: clampSetting(
                      'gifColors',
                      Number(event.target.value)
                    ),
                  })
                }
              />
              <span className={readout}>{settings.gifColors}</span>
            </Row>
            <p className='-mt-1 text-xs text-dim'>
              Fewer colours make a smaller GIF; 64 to 128 is often plenty.
            </p>
            <Row label='Dithering' htmlFor={ditherId} wide>
              <select
                id={ditherId}
                className={field}
                value={settings.gifDither}
                onChange={(event) =>
                  update({
                    gifDither: event.target.value as AdvancedSettings['gifDither'],
                  })
                }
              >
                {gifDitherOptions.map(({ value, label }) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </select>
            </Row>
            <Row label='Plays' htmlFor={playsId} wide>
              <select
                id={playsId}
                className={field}
                value={settings.plays}
                onChange={(event) => update({ plays: Number(event.target.value) })}
              >
                {playsOptions.map(({ value, label }) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </select>
            </Row>
          </div>
        )}
      </section>
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className='flex flex-col gap-[9px] border-b border-line-subtle px-3.5 pb-3 pt-2.5'>
      <h3 className='text-xs font-semibold uppercase tracking-[.07em] text-dim'>
        {title}
      </h3>
      {children}
    </section>
  );
}

/** The board's settings row: a 78 px label, the control, and a 58 px value box unless `wide`. */
function Row({
  label,
  htmlFor,
  wide = false,
  children,
}: {
  label: string;
  htmlFor: string;
  wide?: boolean;
  children: ReactNode;
}) {
  return (
    <div
      className={clsx(
        'grid items-center gap-2',
        wide ? 'grid-cols-[78px_minmax(0,1fr)]' : 'grid-cols-[78px_minmax(0,1fr)_58px]'
      )}
    >
      <label htmlFor={htmlFor} className='text-md text-fg-label'>
        {label}
      </label>
      {children}
    </div>
  );
}

/** Holds a draft while typing so a half-typed width is never clamped. */
function WidthField({
  value,
  max,
  onCommit,
}: {
  value: number | null;
  max: number;
  onCommit: (width: number) => void;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const commit = () => {
    if (draft === null) return;
    const parsed = Number(draft);
    setDraft(null);
    if (draft.trim() !== '' && Number.isFinite(parsed))
      onCommit(Math.min(max, clampSetting('width', parsed)));
  };
  return (
    <input
      type='number'
      aria-label='Width in pixels'
      className='h-6 w-[58px] rounded-[5px] border border-line-control bg-canvas px-1.5 text-right font-mono text-sm tabular-nums text-fg focus:border-accent focus:outline-none'
      min={settingLimits.width.min}
      max={max}
      step={2}
      value={draft ?? value ?? ''}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={commit}
      onKeyDown={(event) => {
        if (event.key === 'Enter') commit();
        if (event.key === 'Escape') setDraft(null);
      }}
    />
  );
}
