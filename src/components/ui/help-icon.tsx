// SPDX-License-Identifier: Apache-2.0

import { HelpCircle } from 'lucide-react';
import type { ReactNode } from 'react';
import { Popover, PopoverContent, PopoverTrigger } from './popover';

interface HelpIconProps {
  content: ReactNode;
  side?: 'top' | 'bottom' | 'left' | 'right';
}

export function HelpIcon({ content, side = 'bottom' }: HelpIconProps) {
  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          className="inline-flex items-center justify-center text-muted-foreground/50 hover:text-muted-foreground transition-colors focus:outline-none focus-visible:ring-1 focus-visible:ring-ring rounded-sm"
          aria-label="Help"
        >
          <HelpCircle size={14} />
        </button>
      </PopoverTrigger>
      <PopoverContent side={side} className="max-w-62 text-sm p-3 w-auto">
        {content}
      </PopoverContent>
    </Popover>
  );
}
