// SPDX-License-Identifier: Apache-2.0

import type { TFunction } from 'i18next';
import type { DdlWarning } from '@/lib/ddl';

export function translateDdlWarning(t: TFunction, w: DdlWarning): string {
  return t(`ddlWarnings.${w.code}`, w.params ?? {});
}
