// SPDX-License-Identifier: BUSL-1.1

import type { SVGProps } from 'react';
import { cn } from '@/lib/utils';

interface QoreAiMarkProps extends Omit<SVGProps<SVGSVGElement>, 'width' | 'height'> {
  size?: number;
  compact?: boolean;
}

/** Qore AI master mark. The compact form is the approved micro-mark for 12–24 px use. */
export function QoreAiMark({ size = 24, compact = false, className, ...props }: QoreAiMarkProps) {
  if (compact) {
    return (
      <svg
        viewBox="0 0 32 32"
        width={size}
        height={size}
        fill="none"
        aria-hidden="true"
        className={cn('shrink-0 text-[var(--q-accent)]', className)}
        {...props}
      >
        <path
          d="M15 3 26 9.5v12L15 28 4 21.5v-12L15 3Z"
          stroke="currentColor"
          strokeWidth="3.25"
          strokeLinejoin="round"
        />
        <circle
          cx="21.5"
          cy="21"
          r="5.5"
          fill="var(--q-bg-0)"
          stroke="currentColor"
          strokeWidth="2.75"
        />
        <path d="m25.5 25 3 3" stroke="currentColor" strokeWidth="2.75" strokeLinecap="round" />
        <path
          d="M21.5 17.5c.43 2.15 1.35 3.07 3.5 3.5-2.15.43-3.07 1.35-3.5 3.5-.43-2.15-1.35-3.07-3.5-3.5 2.15-.43 3.07-1.35 3.5-3.5Z"
          fill="var(--q-ai-signal)"
        />
      </svg>
    );
  }

  return (
    <svg
      viewBox="0 0 128 128"
      width={size}
      height={size}
      fill="none"
      role="img"
      aria-label="Qore AI"
      className={cn('shrink-0 text-[var(--q-accent)]', className)}
      {...props}
    >
      <path
        d="M64 8 112 36v56l-48 28-48-28V36L64 8Z"
        stroke="currentColor"
        strokeWidth="10"
        strokeLinejoin="round"
      />
      <path
        d="M64 27 95 45v37L64 100 33 82V45l31-18Z"
        stroke="var(--q-accent-strong)"
        strokeWidth="7"
        strokeLinejoin="round"
      />
      <path
        d="m64 37 23 13v27L64 90 41 77V50l23-13Z"
        stroke="var(--q-ai-signal)"
        strokeWidth="3"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeDasharray="3 7"
      />
      <path d="m64 47 15 8.5L64 64l-15-8.5L64 47Z" fill="var(--q-accent-strong)" />
      <path d="m49 55.5 15 8.5v19L49 74.5v-19Z" fill="currentColor" />
      <path d="M79 55.5 64 64v19l15-8.5v-19Z" fill="var(--q-accent-strong)" />
      <path d="M64 68v10" stroke="var(--q-bg-0)" strokeWidth="3" strokeLinecap="round" />
      <circle cx="96" cy="95" r="17" stroke="var(--q-bg-0)" strokeWidth="10" />
      <path d="m107.5 106.5 9 9" stroke="var(--q-bg-0)" strokeWidth="10" strokeLinecap="round" />
      <circle cx="96" cy="95" r="17" stroke="currentColor" strokeWidth="6" />
      <path d="m107.5 106.5 9 9" stroke="currentColor" strokeWidth="6" strokeLinecap="round" />
      <path
        d="M96 86c1.12 5.62 3.38 7.88 9 9-5.62 1.12-7.88 3.38-9 9-1.12-5.62-3.38-7.88-9-9 5.62-1.12 7.88-3.38 9-9Z"
        fill="var(--q-ai-signal)"
      />
    </svg>
  );
}
