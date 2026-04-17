// SPDX-License-Identifier: Apache-2.0

import * as React from 'react';
import { cn } from '@/lib/utils';

const Input = React.forwardRef<HTMLInputElement, React.ComponentProps<'input'>>(
  ({ className, type, ...props }, ref) => {
    return (
      <input
        type={type}
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
        className={cn(
          'flex h-9 w-full rounded-md border border-(--color-border) bg-(--color-bg-0) px-3 py-1 text-sm text-(--color-text-0) shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-(--color-accent) disabled:cursor-not-allowed disabled:opacity-50 aria-[invalid=true]:border-[var(--q-error)] aria-[invalid=true]:focus-visible:ring-[var(--q-error)]',
          className
        )}
        ref={ref}
        {...props}
      />
    );
  }
);
Input.displayName = 'Input';

export { Input };
