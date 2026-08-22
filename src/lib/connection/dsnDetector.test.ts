// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';
import { Driver } from './drivers';
import { detectDriverFromDsn } from './dsnDetector';

describe('detectDriverFromDsn', () => {
  it('detects managed Postgres hosts', () => {
    expect(detectDriverFromDsn('postgresql://u:p@db.abc.supabase.co:5432/postgres')?.driver).toBe(
      Driver.Supabase
    );
    expect(
      detectDriverFromDsn('postgresql://u:p@ep-x.eu-central-1.aws.neon.tech/neondb')?.driver
    ).toBe(Driver.Neon);
  });

  it('detects PlanetScale from its gateway host', () => {
    expect(
      detectDriverFromDsn('mysql://u:p@aws.connect.psdb.cloud:3306/app?ssl-mode=required')?.driver
    ).toBe(Driver.PlanetScale);
  });

  it('detects DocumentDB from the AWS service host', () => {
    expect(
      detectDriverFromDsn(
        'mongodb://u:p@docdb-2026.cluster-abc.eu-west-1.docdb.amazonaws.com:27017/app?tls=true'
      )?.driver
    ).toBe(Driver.DocumentDb);
  });

  /** A plain MySQL/Mongo host must keep whatever driver the user picked. */
  it('returns null for a self-hosted host', () => {
    expect(detectDriverFromDsn('mysql://u:p@localhost:3306/app')).toBeNull();
    expect(detectDriverFromDsn('mongodb://u:p@10.0.0.4:27017/app')).toBeNull();
    expect(detectDriverFromDsn('')).toBeNull();
  });
});
