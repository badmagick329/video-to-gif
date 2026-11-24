// shadcn/ui's Switch, as mgck has it, with its theme tokens swapped for the -dg colours every
// caller passed anyway.
import * as SwitchPrimitives from '@radix-ui/react-switch';
import clsx from 'clsx';
import * as React from 'react';

const Switch = React.forwardRef<
  React.ElementRef<typeof SwitchPrimitives.Root>,
  React.ComponentPropsWithoutRef<typeof SwitchPrimitives.Root>
>(({ className, ...props }, ref) => (
  <SwitchPrimitives.Root
    className={clsx(
      'peer inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full border-2 border-transparent shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-foreground-dg focus-visible:ring-offset-2 focus-visible:ring-offset-background-dg disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:bg-primary-dg data-[state=unchecked]:bg-foreground-dg/20',
      className
    )}
    {...props}
    ref={ref}
  >
    <SwitchPrimitives.Thumb className='pointer-events-none block h-4 w-4 rounded-full bg-background-dg shadow-lg ring-0 transition-transform data-[state=checked]:translate-x-4 data-[state=unchecked]:translate-x-0' />
  </SwitchPrimitives.Root>
));
Switch.displayName = SwitchPrimitives.Root.displayName;

export { Switch };
