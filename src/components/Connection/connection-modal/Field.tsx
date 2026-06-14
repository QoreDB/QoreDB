// SPDX-License-Identifier: Apache-2.0

import { cloneElement, type ReactElement, useId } from 'react';

import { Label } from '@/components/ui/label';
import { cn } from '@/lib/utils';

interface FieldProps {
  label: string;
  children: ReactElement<{ id?: string }>;
  required?: boolean;
  hint?: string;
  className?: string;
  labelClassName?: string;
}

/**
 * Associates a label with its control via a generated id (htmlFor/id) so that
 * clicking the label focuses the field and screen readers announce the pair.
 * The child must forward the injected `id` to a focusable element.
 */
export function Field({ label, children, required, hint, className, labelClassName }: FieldProps) {
  const id = useId();

  return (
    <div className={cn('space-y-2', className)}>
      <Label htmlFor={id} className={labelClassName}>
        {label}
        {required && <span className="text-error"> *</span>}
      </Label>
      {cloneElement(children, { id })}
      {hint && <p className="text-xs text-muted-foreground">{hint}</p>}
    </div>
  );
}
