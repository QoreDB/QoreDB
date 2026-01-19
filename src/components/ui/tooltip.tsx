import { ReactNode } from 'react';
import { cn } from '@/lib/utils';

type TooltipSide = 'top' | 'bottom' | 'left' | 'right';

interface TooltipProps {
  content: ReactNode;
  side?: TooltipSide;
  className?: string;
  children: ReactNode;
}

function sideClasses(side: TooltipSide) {
  switch (side) {
    case 'bottom':
      return 'top-full mt-2 left-1/2 -translate-x-1/2';
    case 'left':
      return 'right-full mr-2 top-1/2 -translate-y-1/2';
    case 'right':
      return 'left-full ml-2 top-1/2 -translate-y-1/2';
    case 'top':
    default:
      return 'bottom-full mb-2 left-1/2 -translate-x-1/2';
  }
}

export function Tooltip({ content, side = 'top', className, children }: TooltipProps) {
  if (!content) return <>{children}</>;

  return (
    <span className="relative inline-flex group">
      {children}
      <span
        role="tooltip"
        className={cn(
          'pointer-events-none absolute z-50 whitespace-nowrap rounded-md border border-border bg-background px-2 py-1 text-[11px] text-foreground shadow-md',
          'opacity-0 translate-y-0.5 transition-opacity transition-transform duration-150',
          'group-hover:opacity-100 group-hover:translate-y-0',
          'group-focus-within:opacity-100 group-focus-within:translate-y-0',
          sideClasses(side),
          className
        )}
      >
        {content}
      </span>
    </span>
  );
}

