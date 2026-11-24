import { useEffect, useRef } from 'react';

/** Keys as tokens; `+` and `/` are separators drawn as plain text. */
const shortcuts: Array<[keys: string[], action: string]> = [
  [['Space'], 'Play or pause'],
  [['←', '/', '→'], 'Previous or next frame'],
  [['Shift', '+', '←', '/', '→'], 'Back or forward 1 second'],
  [['I', '/', 'O'], 'Set the selection start or end at the playhead'],
  [['Home', '/', 'End'], 'Go to the selection start or end'],
  [['Ctrl', '+', 'Enter'], 'Export'],
  [['Ctrl', '+', 'O'], 'Open a video'],
  [['?'], 'Show these shortcuts'],
];
const separators = new Set(['+', '/']);

/** The keyboard shortcuts, over the window. `?` or the status bar opens it; Escape closes it. */
export default function ShortcutsOverlay({ onClose }: { onClose: () => void }) {
  const close = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    close.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        onClose();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [onClose]);

  return (
    <div
      className='fixed inset-0 z-50 grid place-items-center bg-black/60 p-4'
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        role='dialog'
        aria-modal='true'
        aria-labelledby='shortcuts-heading'
        className='flex w-full max-w-md flex-col gap-3 rounded-lg border border-line-control bg-panel p-4 shadow-2xl'
      >
        <div className='flex items-center justify-between'>
          <h2 id='shortcuts-heading' className='font-semibold'>
            Keyboard shortcuts
          </h2>
          <button
            ref={close}
            type='button'
            className='inline-flex h-7 items-center rounded-md px-2.5 text-md text-fg-soft hover:bg-hover hover:text-fg'
            onClick={onClose}
          >
            Close
          </button>
        </div>
        <dl className='grid grid-cols-[auto_1fr] items-center gap-x-4 gap-y-2 text-md'>
          {shortcuts.map(([keys, action]) => (
            <div key={action} className='contents'>
              <dt className='flex items-center gap-1 text-dim'>
                {keys.map((key, index) =>
                  separators.has(key) ? (
                    <span key={index}>{key}</span>
                  ) : (
                    <kbd key={index} className='kbd text-fg'>
                      {key}
                    </kbd>
                  )
                )}
              </dt>
              <dd className='text-fg-label'>{action}</dd>
            </div>
          ))}
        </dl>
      </div>
    </div>
  );
}
