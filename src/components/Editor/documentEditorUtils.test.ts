// SPDX-License-Identifier: BUSL-1.1

import { expect, it } from 'vitest';
import { changedDocumentFields } from './documentEditorUtils';

it('does not resend masked fields or nested documents when another field changes', () => {
  const initial = {
    _id: { $oid: 'abc' },
    email: '••••••',
    profile: { token: '••••••' },
    city: 'Paris',
  };
  expect(changedDocumentFields(initial, { ...initial, city: 'Lyon' })).toEqual({ city: 'Lyon' });
  expect(changedDocumentFields(initial, JSON.parse(JSON.stringify(initial)))).toEqual({});
});

it('preserves changed objects and explicit nulls for backend validation', () => {
  expect(
    changedDocumentFields(
      { profile: { email: '••••••', city: 'Paris' }, name: 'A' },
      {
        profile: { email: '••••••', city: 'Lyon' },
        name: null,
      }
    )
  ).toEqual({ profile: { email: '••••••', city: 'Lyon' }, name: null });
});
