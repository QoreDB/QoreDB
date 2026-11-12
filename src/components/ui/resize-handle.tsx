// SPDX-License-Identifier: Apache-2.0

import type * as React from 'react';
import { cn } from '@/lib/utils';

interface ResizeHandleProps {
  /** Orientation of the separator line: `vertical` splits columns. */
  orientation: 'vertical' | 'horizontal';
  label: string;
  value: number;
  min: number;
  max: number;
  controls?: string;
  onMouseDown: (event: React.MouseEvent) => void;
  onDoubleClick: () => void;
  onKeyDown: (event: React.KeyboardEvent) => void;
  className?: string;
  children?: React.ReactNode;
}

export function ResizeHandle({
  orientation,
  label,
  value,
  min,
  max,
  controls,
  onMouseDown,
  onDoubleClick,
  onKeyDown,
  className,
  children,
}: ResizeHandleProps) {
  return (
    // biome-ignore lint/a11y/useSemanticElements: a focusable window splitter has no native element; <hr> cannot take focus or a value
    <div
      role="separator"
      tabIndex={0}
      aria-orientation={orientation}
      aria-label={label}
      aria-valuenow={Math.round(value)}
      aria-valuemin={Math.round(min)}
      aria-valuemax={Math.round(Math.max(min, max))}
      aria-controls={controls}
      onMouseDown={onMouseDown}
      onDoubleClick={onDoubleClick}
      onKeyDown={onKeyDown}
      className={cn('outline-none', className)}
    >
      {children}
    </div>
  );
}
