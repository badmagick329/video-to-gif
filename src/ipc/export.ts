import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type { AdvancedSettings, FileType } from '../editor/lib/advanced';
import type { ClipCrop, ClipTrim, ExportRequest } from '../editor/lib/types';
import type { SourceInfo } from './probe';

// Mirrors of the export slice's IPC types in src-tauri/src/export/domain/ (outcome.rs, memory.rs,
// plan.rs); change them together. `ExportRequest` lives with the editor, which builds it.

/** One ticked file type's result. Failed and cancelled outputs leave no file behind. */
export type ExportOutput =
  | {
      status: 'done';
      fileType: FileType;
      path: string;
      /** Bytes. */
      size: number;
      /** The encoded frame size in pixels. */
      width: number;
      height: number;
    }
  | { status: 'failed'; fileType: FileType; reason: string }
  | { status: 'cancelled'; fileType: FileType };

export type ExportProgress = {
  fileType: FileType;
  /**
   * From 0 to 1 for this file type. GIF, Reverse and Boomerang hold frames until the input ends,
   * so they sit near 0 and then jump.
   */
  progress: number;
};

export type ExportError =
  | { kind: 'notFound'; path: string }
  | { kind: 'destinationNotFound'; path: string }
  | { kind: 'invalidRequest'; reason: string }
  | { kind: 'ffmpegMissing' }
  | { kind: 'notEnoughMemory'; heldBytes: number; availableBytes: number }
  | { kind: 'busy' }
  | { kind: 'captionsFailed'; reason: string };

/** What decides an export's frames, for the memory estimate. */
export type MemoryRequest = {
  info: SourceInfo;
  trim?: ClipTrim;
  crop?: ClipCrop;
  settings: AdvancedSettings;
};

export type MemoryEstimate = {
  /** Peak bytes of frames the export holds; 0 when frames stream through. */
  heldBytes: number;
  /** Free physical memory now, or null where the platform does not say. */
  availableBytes: number | null;
  /** The longest selection, in seconds, that fits with these settings; null when unlimited. */
  limitSeconds: number | null;
  /** False when `runExport` would refuse with `notEnoughMemory`. */
  fits: boolean;
};

/**
 * Exports into the folder `destination`, one output per ticked file type in the order GIF, WebP,
 * MP4. Progress arrives through onExportProgress and each output through onExportOutput as it
 * settles; resolves with every output once the last has settled. Rejects with an ExportError when
 * the export cannot start.
 */
export function runExport(
  request: ExportRequest,
  destination: string
): Promise<ExportOutput[]> {
  return invoke<ExportOutput[]>('run_export', { request, destination });
}

/** Stops the running export; its remaining outputs settle as `cancelled`. */
export function cancelExport(): Promise<void> {
  return invoke<void>('cancel_export');
}

/** Rejects with an ExportError (`invalidRequest`) for an impossible request. */
export function estimateExportMemory(
  request: MemoryRequest
): Promise<MemoryEstimate> {
  return invoke<MemoryEstimate>('estimate_export_memory', { request });
}

export function onExportProgress(
  handler: (progress: ExportProgress) => void
): Promise<UnlistenFn> {
  return listen<ExportProgress>('export-progress', (event) =>
    handler(event.payload)
  );
}

export function onExportOutput(
  handler: (output: ExportOutput) => void
): Promise<UnlistenFn> {
  return listen<ExportOutput>('export-output', (event) =>
    handler(event.payload)
  );
}

export function isExportError(error: unknown): error is ExportError {
  return typeof error === 'object' && error !== null && 'kind' in error;
}

export type OutputError =
  | { kind: 'notAnOutput'; path: string }
  | { kind: 'missing'; path: string }
  | { kind: 'shellFailed'; reason: string };

export type DestinationError =
  | { kind: 'notFound'; path: string }
  | { kind: 'notSaved'; reason: string };

/**
 * Opens an output in its default app. Rejects with an OutputError for a path this session did
 * not produce or that has since gone.
 */
export function openOutput(path: string): Promise<void> {
  return invoke<void>('open_output', { path });
}

/** Opens Explorer on the output's folder with the output selected; rejects like openOutput. */
export function showOutputInFolder(path: string): Promise<void> {
  return invoke<void>('show_output_in_folder', { path });
}

/**
 * The folder picked with Save to on this or an earlier launch, or null when none was picked or
 * it no longer exists (outputs then go beside the source).
 */
export function savedDestination(): Promise<string | null> {
  return invoke<string | null>('saved_destination');
}

/**
 * Remembers `folder` as the destination across launches, or forgets it with null. Rejects with a
 * DestinationError.
 */
export function rememberDestination(folder: string | null): Promise<void> {
  return invoke<void>('remember_destination', { folder });
}

export function isOutputError(error: unknown): error is OutputError {
  return typeof error === 'object' && error !== null && 'kind' in error;
}
