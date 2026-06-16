// SPDX-License-Identifier: Apache-2.0

import { Eye, EyeOff } from 'lucide-react';
import * as React from 'react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Input } from '@/components/ui/input';
import { cn } from '@/lib/utils';

type PasswordInputProps = Omit<React.ComponentProps<'input'>, 'type'>;

export const PasswordInput = React.forwardRef<HTMLInputElement, PasswordInputProps>(
  ({ className, ...props }, ref) => {
    const { t } = useTranslation();
    const [visible, setVisible] = useState(false);

    return (
      <div className="relative">
        <Input
          ref={ref}
          type={visible ? 'text' : 'password'}
          className={cn('pr-9', className)}
          {...props}
        />
        <button
          type="button"
          onClick={() => setVisible(v => !v)}
          aria-label={visible ? t('connection.hidePassword') : t('connection.showPassword')}
          className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground transition-colors hover:text-foreground"
        >
          {visible ? <EyeOff size={16} /> : <Eye size={16} />}
        </button>
      </div>
    );
  }
);
PasswordInput.displayName = 'PasswordInput';
