import { open } from '@tauri-apps/plugin-dialog';
import { useCallback, useEffect, useState } from 'react';

import { rememberDestination, savedDestination } from '../ipc/export';

/** Where Convert puts its outputs, and why. */
export type Destination = {
  folder: string;
  /** `source`: beside the video, the default; `chosen`: picked with Save to. */
  from: 'source' | 'chosen';
};

/**
 * The folder holding `path`. A file at a drive's root gives `C:\`, since `C:` alone means the
 * drive's current directory to Windows.
 */
export function sourceFolder(path: string): string {
  const cut = Math.max(path.lastIndexOf('\\'), path.lastIndexOf('/'));
  if (cut < 0) return path;
  const folder = path.slice(0, cut);
  return /^[A-Za-z]:$/.test(folder) ? `${folder}${path[cut]}` : folder;
}

/**
 * The destination for the open source: the folder picked with Save to, remembered across
 * launches by Rust, or else the source's own folder. Rust offers a remembered folder only while
 * it exists, so one that was deleted falls back to the source's folder.
 */
export function useDestination(sourcePath: string | null) {
  const [chosen, setChosen] = useState<string | null>(null);
  const [error, setError] = useState('');

  useEffect(() => {
    let current = true;
    savedDestination().then(
      (folder) => {
        if (current && folder) setChosen((picked) => picked ?? folder);
      },
      // Nothing remembered is the default anyway.
      () => {}
    );
    return () => {
      current = false;
    };
  }, []);

  const destination: Destination | null = chosen
    ? { folder: chosen, from: 'chosen' }
    : sourcePath
      ? { folder: sourceFolder(sourcePath), from: 'source' }
      : null;

  const choose = useCallback(async () => {
    const folder = await open({
      directory: true,
      title: 'Save to',
      defaultPath: destination?.folder,
    });
    if (typeof folder !== 'string') return;
    setError('');
    try {
      await rememberDestination(folder);
      setChosen(folder);
    } catch {
      setError(`Could not use ${folder} as the folder to save to.`);
    }
  }, [destination?.folder]);

  const saveBesideSource = useCallback(async () => {
    setError('');
    setChosen(null);
    // Forgetting can only fail to delete a small file; the session already uses the default.
    await rememberDestination(null).catch(() => {});
  }, []);

  return { destination, error, choose, saveBesideSource };
}
