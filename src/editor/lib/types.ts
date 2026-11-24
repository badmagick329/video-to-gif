// The editor's shared types, from mgck's `lib/types/discordgifs.ts`, less its simple-mode and
// ffmpeg.wasm types.
import type { AdvancedSettings, FileType } from './advanced';

export type ClipTrim = { start: number; end: number };
/**
 * Fractions of the source's displayed frame. They keep crops independent of the player's size
 * and of a proxy's resolution; pixel figures come from multiplying by the source's displayed
 * size (`croppedSourceSize`, `cropPixels`), never by the player's.
 */
export type ClipCrop = { x: number; y: number; width: number; height: number };

/** One file an export made, kept for the rest of the session. */
export type SessionResult =
  | { id: string; fileType: FileType; name: string; url: string; size: number }
  | { id: string; fileType: FileType; failed: true; reason?: string };

/**
 * What the Convert button asks for: mgck's single-pass job, with the source as a path. Export
 * (VTG-6) turns it into ffmpeg arguments in Rust.
 */
export type ExportRequest = {
  path: string;
  trim?: ClipTrim;
  crop?: ClipCrop;
  /** `fileTypes` in `fileTypeOrder`. */
  settings: AdvancedSettings;
  /** The exact output width, even and capped at the turned crop. */
  width: number;
  keepSound: boolean;
  /** The captions as a transparent PNG at the exact output size, when there are any. */
  captions?: Uint8Array;
  outputSeconds: number;
};
