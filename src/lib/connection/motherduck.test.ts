// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import { motherDuckHostFromToken, resolveMotherDuckHost } from './motherduck';

const EU_TOKEN = 'header.eyJtZFJlZ2lvbiI6ImF3cy1ldS1jZW50cmFsLTEifQ.signature';

describe('MotherDuck endpoint resolution', () => {
  it('derives the regional PostgreSQL endpoint from a token', () => {
    expect(motherDuckHostFromToken(EU_TOKEN)).toBe('pg.eu-central-1-aws.motherduck.com');
  });

  it('replaces a mismatched official endpoint', () => {
    expect(resolveMotherDuckHost('pg.us-east-1-aws.motherduck.com', EU_TOKEN)).toBe(
      'pg.eu-central-1-aws.motherduck.com'
    );
  });

  it('preserves custom endpoints and opaque tokens', () => {
    expect(resolveMotherDuckHost('localhost', EU_TOKEN)).toBe('localhost');
    expect(resolveMotherDuckHost('pg.us-east-1-aws.motherduck.com', 'opaque-token')).toBe(
      'pg.us-east-1-aws.motherduck.com'
    );
  });
});
