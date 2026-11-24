import { useEffect, useState } from 'react';

import { fileUrl, makeThumbnails } from '../ipc/preview';

/**
 * The trim timeline's thumbnails for the open source at `path`, as URLs the webview can load, or
 * none while ffmpeg makes them or when it fails: a plain bar reads better than a broken strip.
 *
 * mgck draws them by seeking a hidden `<video>`, which took 20 s on a long 4K source here;
 * ffmpeg makes the whole strip in one pass (`make_thumbnails`). `src`, the preview source,
 * re-requests them when a proxy replaces the source, so the strip follows what the player shows.
 */
export function useTimelineThumbnails(path: string, src: string) {
  // Frames are kept with the request they answer, so a new one shows none until its own
  // arrive, without resetting state in the effect.
  const key = `${path}|${src}`;
  const [strip, setStrip] = useState<{ key: string; frames: string[] }>({
    key,
    frames: [],
  });

  useEffect(() => {
    let current = true;
    makeThumbnails(path).then(
      (files) => {
        if (current) setStrip({ key, frames: files.map(fileUrl) });
      },
      () => {
        // The timeline works without thumbnails; nothing to tell the user.
      }
    );
    return () => {
      current = false;
    };
  }, [path, key]);

  return strip.key === key ? strip.frames : [];
}
