import { ReactNode, useId, useState } from 'react';

import RunningCard from './RunningCard';
import SessionResults from './SessionResults';
import type { ExportJob } from './lib/export-run';
import type { SessionResult } from './lib/types';

type Tab = 'output' | 'exports';

/**
 * The docked panel on the window's right, spanning the viewer and the timeline. The Output tab
 * holds the output settings over the pinned export footer; the Exports tab holds the running
 * export's card and what this session made. Only the tab's body scrolls.
 */
export default function Inspector({
  output,
  footer,
  results,
  job,
}: {
  /** The Output tab's settings, or what stands in for them with nothing open. */
  output: ReactNode;
  /** The export footer, pinned under the Output tab's settings. */
  footer?: ReactNode;
  results: readonly SessionResult[];
  /** The running export, which may have started before this clip was opened. */
  job: ExportJob | null;
}) {
  const [tab, setTab] = useState<Tab>('output');
  const id = useId();
  const tabs: Array<{ value: Tab; label: string; badge?: number }> = [
    { value: 'output', label: 'Output' },
    { value: 'exports', label: 'Exports', badge: results.length },
  ];
  return (
    <aside
      aria-label='Inspector'
      className='flex min-h-0 min-w-0 flex-col border-l border-line bg-panel'
    >
      <div
        role='tablist'
        aria-label='Inspector'
        className='flex h-9 shrink-0 items-stretch gap-0.5 border-b border-line px-2'
      >
        {tabs.map(({ value, label, badge }) => (
          <button
            key={value}
            type='button'
            role='tab'
            id={`${id}-${value}`}
            aria-selected={tab === value}
            aria-controls={`${id}-${value}-panel`}
            className='flex items-center border-b-2 border-transparent px-2.5 text-md text-muted aria-selected:border-accent aria-selected:font-medium aria-selected:text-white'
            onClick={() => setTab(value)}
          >
            {label}
            {badge !== undefined && (
              <span
                aria-label={`${badge} ${badge === 1 ? 'item' : 'items'}`}
                className='ml-1 inline-block min-w-4 rounded-lg bg-line-control px-1 text-center text-2xs text-fg-label'
              >
                {badge}
              </span>
            )}
          </button>
        ))}
      </div>
      {/* Both panels stay mounted, so the Output tab keeps its open sections and the footer
          its last notice while the Exports tab is shown. */}
      <div
        role='tabpanel'
        id={`${id}-output-panel`}
        aria-labelledby={`${id}-output`}
        hidden={tab !== 'output'}
        // Tailwind's display utilities outrank the hidden attribute, so the class follows it too.
        className={tab === 'output' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
      >
        <div className='min-h-0 flex-1 overflow-y-auto pb-3 pt-1'>{output}</div>
        {footer}
      </div>
      <div
        role='tabpanel'
        id={`${id}-exports-panel`}
        aria-labelledby={`${id}-exports`}
        hidden={tab !== 'exports'}
        className='flex min-h-0 flex-1 flex-col gap-3.5 overflow-y-auto p-3'
      >
        {job && <RunningCard job={job} />}
        <SessionResults results={results} />
      </div>
    </aside>
  );
}
