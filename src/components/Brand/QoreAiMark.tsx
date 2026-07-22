// SPDX-License-Identifier: BUSL-1.1

import type { HTMLAttributes, SVGProps } from 'react';
import { cn } from '@/lib/utils';

interface QoreAiMarkProps extends Omit<SVGProps<SVGSVGElement>, 'width' | 'height'> {
  size?: number;
}

/** Full-colour Qore AI master mark for hero and brand moments. */
export function QoreAiMark({ size = 24, className, ...props }: QoreAiMarkProps) {
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
      <path d="m106.5 105.5 10 10" stroke="var(--q-bg-0)" strokeWidth="12" strokeLinecap="round" />
      <circle cx="96" cy="95" r="20" fill="var(--q-bg-0)" />
      <path d="m106.5 105.5 10 10" stroke="currentColor" strokeWidth="7" strokeLinecap="round" />
      <circle cx="96" cy="95" r="17" fill="currentColor" />
      <circle cx="96" cy="95" r="11" fill="var(--q-bg-0)" />
      <path
        d="M96 86c1.12 5.62 3.38 7.88 9 9-5.62 1.12-7.88 3.38-9 9-1.12-5.62-3.38-7.88-9-9 5.62-1.12 7.88-3.38 9-9Z"
        fill="var(--q-ai-signal)"
      />
    </svg>
  );
}

interface QoreAiMonoMarkProps extends Omit<HTMLAttributes<HTMLSpanElement>, 'children'> {
  size?: number;
}

/** Monochrome Qore AI mark for navigation, menus and compact status UI. */
export function QoreAiMonoMark({ size = 16, className, style, ...props }: QoreAiMonoMarkProps) {
  return (
    <span
      aria-hidden="true"
      className={cn('inline-block shrink-0', className)}
      style={{
        width: size,
        height: size,
        backgroundColor: 'currentColor',
        WebkitMask: "url('/brand/qore-ai-mark-mono-light.svg') center / contain no-repeat",
        mask: "url('/brand/qore-ai-mark-mono-light.svg') center / contain no-repeat",
        ...style,
      }}
      {...props}
    />
  );
}
