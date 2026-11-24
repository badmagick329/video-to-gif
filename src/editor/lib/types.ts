// The editor's shared types, from mgck's `lib/types/discordgifs.ts`, less its simple-mode and
// ffmpeg.wasm types.
import type { SourceInfo } from '../../ipc/probe';
import type { AdvancedSettings, FileType } from './advanced';

export type ClipTrim = { start: number; end: number };
/**
 * Fractions of the source's displayed frame. They keep crops independent of the player's size
 * and of a proxy's resolution; pixel figures come from multiplying by the source's displayed
 * size (`croppedSourceSize`, `cropPixels`), never by the player's.
 */
export type ClipCrop = { x: number; y: number; width: number; height: number };

/**
 * One output an export made, or one file type that failed, kept for the rest of the session.
 * `path` is what Rust reported; Open and Show in folder send it back unchanged, since Rust accepts
 * only paths it produced.
 */
export type SessionResult =
  | {
      id: string;
      fileType: FileType;
      path: string;
      name: string;
      /** The asset-protocol URL the list plays it from. */
      url: string;
      size: number;
      width: number;
      height: number;
    }
  /** `reason` is the tail of ffmpeg's error output, or the filesystem's error. */
  | { id: string; fileType: FileType; failed: true; reason?: string };

/**
 * What the Export button asks for: mgck's single-pass job, with the source as a path and its
 * probed info. Rust's `export::domain::plan::ExportRequest` mirrors it; change them together.
 * Rust works out the output size and length from `info`, the crop and the settings itself, with
 * the same functions as `advanced.ts`, and refuses captions drawn at any other size.
 */
export type ExportRequest = {
  path: string;
  /** What `probe_source` returned for `path`. */
  info: SourceInfo;
  trim?: ClipTrim;
  crop?: ClipCrop;
  /** `fileTypes` in `fileTypeOrder`. Caption text and size are not read: `captions` carries them. */
  settings: AdvancedSettings;
  keepSound: boolean;
  /** The captions as a transparent PNG at the exact output size, when there are any. */
  captions?: Uint8Array;
  /**
   * The user saw the frame-memory warning and chose Export anyway. Without it Rust refuses a
   * run whose held frames exceed free memory.
   */
  acceptPaging?: boolean;
};
