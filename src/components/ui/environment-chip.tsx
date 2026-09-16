// SPDX-License-Identifier: Apache-2.0

import { ENVIRONMENT_CONFIG, type Environment } from '@/lib/environment';
import { cn } from '@/lib/utils';

interface EnvironmentChipProps {
  environment: Environment;
  className?: string;
}

/** Short environment tag (DEV / STG / PROD) painted with the environment tokens. */
export function EnvironmentChip({ environment, className }: EnvironmentChipProps) {
  const config = ENVIRONMENT_CONFIG[environment] ?? ENVIRONMENT_CONFIG.development;
  return (
    <span
      className={cn(
        'inline-flex items-center rounded-sm px-1.5 py-px text-[10px] font-semibold tracking-wide',
        className
      )}
      style={{ backgroundColor: config.bgSoft, color: config.color }}
    >
      {config.labelShort}
    </span>
  );
}
