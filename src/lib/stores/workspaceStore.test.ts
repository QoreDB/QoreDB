// SPDX-License-Identifier: Apache-2.0

import { beforeEach, expect, it } from 'vitest';
import {
  captureWorkspaceScope,
  setActiveWorkspace,
  setRecentWorkspaces,
  setWorkspaceLoading,
} from './workspaceStore';

beforeEach(() => setActiveWorkspace(null, 'a'));
it('invalidates an activation through an A/B/A round trip', () => {
  const current = captureWorkspaceScope();
  setActiveWorkspace(null, 'b');
  setActiveWorkspace(null, 'a');
  expect(current()).toBe(false);
  expect(captureWorkspaceScope()()).toBe(true);
});
it('invalidates work as soon as a transition starts, including a failed transition', () => {
  const current = captureWorkspaceScope();
  setWorkspaceLoading(true);
  const duringTransition = captureWorkspaceScope();
  setWorkspaceLoading(false);
  expect(current()).toBe(false);
  expect(duringTransition()).toBe(false);
});
it('does not invalidate an activation for a recent-workspaces refresh', () => {
  const current = captureWorkspaceScope();
  setRecentWorkspaces([]);
  expect(current()).toBe(true);
});
it('rejects a mismatching project from the beginning', () => {
  expect(captureWorkspaceScope('b')()).toBe(false);
});
