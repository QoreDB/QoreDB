// SPDX-License-Identifier: Apache-2.0

export interface TourStepDef {
  targetSelector: string;
  titleKey: string;
  descriptionKey: string;
  position: 'top' | 'bottom' | 'left' | 'right';
}

export const TOUR_FIRST_QUERY: TourStepDef[] = [
  {
    targetSelector: '[data-tour="query-editor"]',
    titleKey: 'tour.firstQuery.step1Title',
    descriptionKey: 'tour.firstQuery.step1Desc',
    position: 'bottom',
  },
  {
    targetSelector: '[data-tour="query-execute"]',
    titleKey: 'tour.firstQuery.step2Title',
    descriptionKey: 'tour.firstQuery.step2Desc',
    position: 'bottom',
  },
  {
    targetSelector: '[data-tour="query-results"]',
    titleKey: 'tour.firstQuery.step3Title',
    descriptionKey: 'tour.firstQuery.step3Desc',
    position: 'top',
  },
];

export const TOUR_FIRST_NOTEBOOK: TourStepDef[] = [
  {
    targetSelector: '[data-tour="notebook-cells"]',
    titleKey: 'tour.firstNotebook.step1Title',
    descriptionKey: 'tour.firstNotebook.step1Desc',
    position: 'right',
  },
  {
    targetSelector: '[data-tour="notebook-add-cell"]',
    titleKey: 'tour.firstNotebook.step2Title',
    descriptionKey: 'tour.firstNotebook.step2Desc',
    position: 'bottom',
  },
  {
    targetSelector: '[data-tour="notebook-save"]',
    titleKey: 'tour.firstNotebook.step3Title',
    descriptionKey: 'tour.firstNotebook.step3Desc',
    position: 'bottom',
  },
];

export const TOUR_FIRST_TABLE: TourStepDef[] = [
  {
    targetSelector: '[data-tour="table-data"]',
    titleKey: 'tour.firstTable.step1Title',
    descriptionKey: 'tour.firstTable.step1Desc',
    position: 'top',
  },
  {
    targetSelector: '[data-tour="table-tabs"]',
    titleKey: 'tour.firstTable.step2Title',
    descriptionKey: 'tour.firstTable.step2Desc',
    position: 'bottom',
  },
];

export const TOUR_WORKSPACES: TourStepDef[] = [
  {
    targetSelector: '[data-tour="workspace-switcher"]',
    titleKey: 'tour.workspaces.step1Title',
    descriptionKey: 'tour.workspaces.step1Desc',
    position: 'right',
  },
  {
    targetSelector: '[data-tour="sidebar-connections"]',
    titleKey: 'tour.workspaces.step2Title',
    descriptionKey: 'tour.workspaces.step2Desc',
    position: 'right',
  },
  {
    targetSelector: '[data-tour="workspace-switcher"]',
    titleKey: 'tour.workspaces.step3Title',
    descriptionKey: 'tour.workspaces.step3Desc',
    position: 'right',
  },
];

export const TOURS: Record<string, TourStepDef[]> = {
  'first-query': TOUR_FIRST_QUERY,
  'first-notebook': TOUR_FIRST_NOTEBOOK,
  'first-table': TOUR_FIRST_TABLE,
  workspaces: TOUR_WORKSPACES,
};
