import { invoke } from '@tauri-apps/api/core';

// Mirrors of the probe slice's IPC types in src-tauri/src/probe/domain.rs; change them together.

export type Tool =
  | { status: 'found'; path: string; version: string }
  | { status: 'missing' }
  | { status: 'unusable'; path: string; reason: string };

export type FfmpegTools = { ffmpeg: Tool; ffprobe: Tool };

export type SourceInfo = {
  /** Seconds. */
  duration: number;
  /** Displayed size: coded size with rotation metadata applied. */
  width: number;
  height: number;
  frameRate: number;
  videoCodec: string;
  /** ffprobe's format_name, e.g. "mov,mp4,m4a,3gp,3g2,mj2". */
  container: string;
  /** ffprobe's codec_name for the first audio stream, e.g. "aac" or "flac"; null when silent. */
  audioCodec: string | null;
};

/** Whether the source has sound; the probe's audio codec is the one field that says so. */
export function hasAudio(info: SourceInfo): boolean {
  return info.audioCodec !== null;
}

export type ProbeError =
  | { kind: 'ffprobeMissing' }
  | { kind: 'ffprobeFailed'; reason: string }
  | { kind: 'notFound'; path: string }
  | { kind: 'unreadable'; path: string; reason: string }
  | { kind: 'noVideoStream'; path: string }
  | { kind: 'invalidOutput'; path: string; reason: string };

export function findFfmpeg(): Promise<FfmpegTools> {
  return invoke<FfmpegTools>('find_ffmpeg');
}

export function isProbeError(error: unknown): error is ProbeError {
  return typeof error === 'object' && error !== null && 'kind' in error;
}

/** Rejects with a ProbeError. */
export function probeSource(path: string): Promise<SourceInfo> {
  return invoke<SourceInfo>('probe_source', { path });
}
