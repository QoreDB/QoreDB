// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import {
  buildMigrationFilename,
  nextVersion,
  parseMigration,
  serializeMigration,
  slugify,
  summarize,
} from './parse';

describe('parseMigration', () => {
  it('splits up and down sections', () => {
    const content = '-- migrate:up\nCREATE TABLE t (id int);\n\n-- migrate:down\nDROP TABLE t;\n';
    expect(parseMigration(content)).toEqual({
      up: 'CREATE TABLE t (id int);',
      down: 'DROP TABLE t;',
    });
  });

  it('treats a marker-less file as the up script', () => {
    expect(parseMigration('SELECT 1;')).toEqual({ up: 'SELECT 1;', down: '' });
  });

  it('handles a down section with no up', () => {
    expect(parseMigration('-- migrate:down\nDROP TABLE t;')).toEqual({
      up: '',
      down: 'DROP TABLE t;',
    });
  });

  it('round-trips through serializeMigration', () => {
    const up = 'CREATE TABLE t (id int);';
    const down = 'DROP TABLE t;';
    expect(parseMigration(serializeMigration(up, down))).toEqual({ up, down });
  });
});

describe('slugify', () => {
  it('normalizes to a filesystem-safe slug', () => {
    expect(slugify('Create Users Table!')).toBe('create_users_table');
  });

  it('falls back when nothing survives', () => {
    expect(slugify('!!!')).toBe('migration');
  });
});

describe('nextVersion', () => {
  const m = (version: string) => ({ version, name: 'x', filename: `${version}_x.sql` });

  it('starts at 0001', () => {
    expect(nextVersion([])).toBe('0001');
  });

  it('increments past the highest version', () => {
    expect(nextVersion([m('0001'), m('0009')])).toBe('0010');
  });

  it('ignores non-numeric versions', () => {
    expect(nextVersion([m('oops'), m('0003')])).toBe('0004');
  });
});

describe('summarize', () => {
  it('splits version and name', () => {
    expect(summarize('0001_create_users.sql')).toEqual({
      version: '0001',
      name: 'create_users',
      filename: '0001_create_users.sql',
    });
  });

  it('keeps a name containing underscores intact', () => {
    expect(summarize('0002_add_user_email.sql').name).toBe('add_user_email');
  });

  it('matches buildMigrationFilename', () => {
    const filename = buildMigrationFilename('0007', 'add_index');
    expect(summarize(filename)).toEqual({
      version: '0007',
      name: 'add_index',
      filename: '0007_add_index.sql',
    });
  });
});
