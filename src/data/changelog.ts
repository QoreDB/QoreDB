// SPDX-License-Identifier: Apache-2.0

export interface ChangelogItem {
  titleKey: string;
  descriptionKey: string;
  type: 'feature' | 'improvement' | 'fix';
  featureId?: string;
}

export interface ChangelogEntry {
  version: string;
  date: string;
  items: ChangelogItem[];
}

/**
 * Changelog entries for the What's New panel.
 * Keep entries in reverse-chronological order (newest first).
 * All title/description values are i18n keys.
 */
export const CHANGELOG: ChangelogEntry[] = [
  {
    version: '0.3.0',
    date: '2026-03-16',
    items: [
      {
        titleKey: 'features.notebooks.name',
        descriptionKey: 'features.notebooks.description',
        type: 'feature',
        featureId: 'feat_notebook',
      },
      {
        titleKey: 'features.federation.name',
        descriptionKey: 'features.federation.description',
        type: 'feature',
        featureId: 'feat_federation',
      },
      {
        titleKey: 'features.snapshots.name',
        descriptionKey: 'features.snapshots.description',
        type: 'feature',
        featureId: 'feat_snapshots',
      },
    ],
  },
];
