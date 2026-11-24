import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type { SourceInfo } from './probe';

// Mirrors of the preview slice's IPC types in src-tauri/src/preview/domain.rs; change them together.

/** What the player plays: the source file itself, or a proxy made by ffmpeg. */
export type PreviewSource = 'source' | 'proxy';

export type ProxyProgress = {
  /** The source path the proxy is for. */
  source: string;
  /** From 0 to 1. */
  progress: number;
};

export type PreviewError =
  | { kind: 'notFound'; path: string }
  | { kind: 'invalidSourceInfo'; reason: string }
  | { kind: 'notServable'; path: string; reason: string }
  | { kind: 'notOpen'; path: string }
  | { kind: 'ffmpegMissing' }
  | { kind: 'proxyFailed'; reason: string }
  | { kind: 'thumbnailsFailed'; reason: string }
  | { kind: 'cancelled' };

/**
 * Opens the source for preview, replacing (and cancelling the proxy of) any open one.
 * Rejects with a PreviewError.
 */
export function openSource(
  path: string,
  info: SourceInfo
): Promise<PreviewSource> {
  return invoke<PreviewSource>('open_source', { path, info });
}

/**
 * Closes the source if it is still the open one: stops its proxy build and deletes its proxy and
 * thumbnails. A path that is no longer open is ignored. Never rejects with a PreviewError.
 */
export function closeSource(path: string): Promise<void> {
  return invoke<void>('close_source', { path });
}

/**
 * Resolves to the proxy's file path once it exists, building it if needed; progress arrives
 * through onProxyProgress. Rejects with a PreviewError, `cancelled` when another source replaced
 * this one.
 */
export function makeProxy(path: string): Promise<string> {
  return invoke<string>('make_proxy', { path });
}

/** Thumbnails in the trim timeline's strip; Rust's `THUMBNAIL_COUNT`. */
export const timelineThumbnailCount = 24;

/**
 * Resolves to the paths of the open source's timeline thumbnails, in time order, made by ffmpeg
 * from the preview source. Rejects with a PreviewError.
 */
export function makeThumbnails(path: string): Promise<string[]> {
  return invoke<string[]>('make_thumbnails', { path });
}

export function onProxyProgress(
  handler: (progress: ProxyProgress) => void
): Promise<UnlistenFn> {
  return listen<ProxyProgress>('proxy-progress', (event) =>
    handler(event.payload)
  );
}

/** URL the webview loads a file from, through Tauri's asset protocol. */
export function fileUrl(path: string): string {
  return convertFileSrc(path);
}

export function isPreviewError(error: unknown): error is PreviewError {
  return typeof error === 'object' && error !== null && 'kind' in error;
}
