// SPDX-License-Identifier: Apache-2.0

import { cn } from '@/lib/utils';

interface SkeletonProps {
  className?: string;
  lines?: number;
}

export function Skeleton({ className, lines = 1 }: SkeletonProps) {
  if (lines <= 1) {
    return <div className={cn('h-4 rounded-md bg-muted animate-pulse', className)} />;
  }
  return (
    <div className="space-y-2">
      {Array.from({ length: lines }).map((_, i) => (
        <div
          key={i}
          className={cn(
            'h-4 rounded-md bg-muted animate-pulse',
            i === lines - 1 && 'w-3/4',
            className
          )}
        />
      ))}
    </div>
  );
}
