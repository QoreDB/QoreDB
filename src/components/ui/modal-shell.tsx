// SPDX-License-Identifier: Apache-2.0

import * as DialogPrimitive from '@radix-ui/react-dialog';
import type * as React from 'react';
import { cn } from '@/lib/utils';

interface ModalShellProps {
  open: boolean;
  onClose: () => void;
  label?: string;
  labelledBy?: string;
  initialFocusRef?: React.RefObject<HTMLElement | null>;
  overlayClassName?: string;
  containerClassName?: string;
  contentClassName?: string;
  children: React.ReactNode;
}

/**
 * Dialog semantics, focus trap, Escape and focus restoration for modals whose
 * layout does not fit `DialogContent` (top-anchored palettes, full panels).
 */
export function ModalShell({
  open,
  onClose,
  label,
  labelledBy,
  initialFocusRef,
  overlayClassName,
  containerClassName,
  contentClassName,
  children,
}: ModalShellProps) {
  return (
    <DialogPrimitive.Root open={open} onOpenChange={next => !next && onClose()}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className={cn('fixed inset-0 z-50', overlayClassName)} />
        <div
          className={cn(
            'pointer-events-none fixed inset-0 z-50 flex justify-center p-4',
            containerClassName
          )}
        >
          <DialogPrimitive.Content
            aria-label={label}
            aria-labelledby={labelledBy}
            aria-describedby={undefined}
            onEscapeKeyDown={event => event.stopPropagation()}
            onOpenAutoFocus={event => {
              if (!initialFocusRef?.current) return;
              event.preventDefault();
              initialFocusRef.current.focus();
            }}
            className={cn('pointer-events-auto outline-none', contentClassName)}
          >
            {children}
          </DialogPrimitive.Content>
        </div>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
