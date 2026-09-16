// SPDX-License-Identifier: BUSL-1.1

import type { Namespace, QueryResult } from '@/lib/tauri';
import { invoke } from '@/lib/transport';

export type CaptureMode = 'full' | 'metadata_only';

/** How a recording treats query text that looks like it carries a credential. */
export type SecretPolicy = 'off' | 'warn' | 'redact';
export type CaptureStopReason = 'budget_exceeded' | 'metadata_only' | 'production_policy';
export type ReplayVerdict =
  | 'match'
  | 'broken'
  | 'row_count_diff'
  | 'digest_diff'
  | 'slower'
  | 'not_compared'
  | 'skipped';

export interface ExpectedOutcome {
  execution_time_ms: number;
  row_count?: number | null;
  success: boolean;
  fingerprint?: string | null;
  result_digest?: string | null;
}

export interface ReplayEntry {
  id: string;
  order: number;
  query: string;
  driver_id: string;
  namespace?: Namespace | null;
  operation_type: string;
  is_mutation: boolean;
  expected: ExpectedOutcome;
}

export interface ReplaySet {
  version: number;
  name: string;
  created_at: string;
  source: { driver_id: string; connection_label?: string | null; environment: string };
  ignored_columns: string[];
  /** Query text was redacted on the way in: shareable, and not replayable. */
  redacted: boolean;
  entries: ReplayEntry[];
}

export interface ReplaySetSummary {
  slug: string;
  name: string;
  created_at: string;
  driver_id: string;
  environment: string;
  entry_count: number;
  redacted: boolean;
}

export interface RunMeta {
  run_id: string;
  set_slug: string;
  set_name: string;
  started_at: string;
  finished_at?: string | null;
  connection_label?: string | null;
  driver_id: string;
  environment: string;
  capture_mode: CaptureMode;
  capture_stopped_reason?: CaptureStopReason | null;
  is_baseline: boolean;
  captured_bytes: number;
  entry_count: number;
}

export interface ReplayEntryResult {
  entry_id: string;
  order: number;
  query_preview: string;
  verdict: ReplayVerdict;
  success: boolean;
  error?: string | null;
  skip_reason?: string | null;
  /** Stable key for `skip_reason`, translated by the UI when present. */
  skip_code?: string | null;
  execution_time_ms: number;
  expected_execution_time_ms: number;
  row_count?: number | null;
  expected_row_count?: number | null;
  digest?: string | null;
  expected_digest?: string | null;
  captured: boolean;
  partial_comparison: boolean;
}

export interface ReplaySummary {
  total: number;
  matched: number;
  broken: number;
  row_count_diff: number;
  digest_diff: number;
  slower: number;
  not_compared: number;
  skipped: number;
}

export interface ReplayReport {
  run: RunMeta;
  baseline_run_id?: string | null;
  results: ReplayEntryResult[];
  summary: ReplaySummary;
}

/** Two runs of the same set on two connections, compared against each other. */
export interface ReplayAbReport {
  left: RunMeta;
  right: RunMeta;
  results: ReplayEntryResult[];
  summary: ReplaySummary;
}

export interface ReplayRunOptions {
  capture_mode: CaptureMode;
  allow_mutations: boolean;
  max_captured_rows: number;
  capture_budget_bytes: number;
  slower_ratio: number;
  slower_min_delta_ms: number;
  run_retention: number;
}

export interface RecordingStatus {
  run_id: string;
  name: string;
  started_at: string;
  entry_count: number;
  captured_bytes: number;
  capture_mode: CaptureMode;
  capture_stopped_reason?: CaptureStopReason | null;
  /** Executions seen from another connection and left out. */
  ignored_other_session: number;
  record_mutations: boolean;
  /** Mutations left out because `record_mutations` is off. */
  excluded_mutations: number;
  /** Recorded entries that write. Zero unless `record_mutations` is on. */
  mutation_count: number;
  /** Recorded queries that look like they carry a credential. */
  secrets_detected: number;
  secret_policy: SecretPolicy;
}

export interface RecordedPreview {
  order: number;
  query_preview: string;
  is_mutation: boolean;
  looks_like_secret: boolean;
}

export interface ReplayProgress {
  run_id: string;
  completed: number;
  total: number;
  current_query_preview: string;
}

export const REPLAY_PROGRESS_EVENT = 'replay-progress';

/** A recording can be stopped from the status bar, outside the Replay tab. */
export const RECORDING_CHANGED_EVENT = 'qore:replay-recording-changed';

export const DEFAULT_RUN_OPTIONS: ReplayRunOptions = {
  capture_mode: 'full',
  allow_mutations: false,
  max_captured_rows: 1000,
  capture_budget_bytes: 64 * 1024 * 1024,
  slower_ratio: 2,
  slower_min_delta_ms: 100,
  run_retention: 10,
};

export interface StartRecordingRequest {
  session_id: string;
  name: string;
  ignored_columns: string[];
  record_mutations: boolean;
  capture_mode: CaptureMode;
  allow_production_capture: boolean;
  secret_policy: SecretPolicy;
}

export async function startRecording(request: StartRecordingRequest): Promise<RecordingStatus> {
  return invoke('replay_start_recording', { request });
}

export async function stopRecording(slug?: string): Promise<ReplaySetSummary> {
  return invoke('replay_stop_recording', { slug: slug ?? null });
}

export async function cancelRecording(): Promise<void> {
  return invoke('replay_cancel_recording');
}

export async function getRecordingStatus(): Promise<RecordingStatus | null> {
  return invoke('replay_recording_status');
}

export async function getRecordedPreviews(): Promise<RecordedPreview[]> {
  return invoke('replay_recorded_previews');
}

export async function discardRecorded(index: number): Promise<void> {
  return invoke('replay_discard_recorded', { index });
}

export async function discardRecordedMutations(): Promise<number> {
  return invoke('replay_discard_mutations');
}

export async function listReplaySets(): Promise<ReplaySetSummary[]> {
  return invoke('replay_list_sets');
}

export async function loadReplaySet(slug: string): Promise<ReplaySet> {
  return invoke('replay_load_set', { slug });
}

export async function deleteReplaySet(slug: string): Promise<void> {
  return invoke('replay_delete_set', { slug });
}

export async function setIgnoredColumns(slug: string, columns: string[]): Promise<ReplaySet> {
  return invoke('replay_set_ignored_columns', { slug, columns });
}

export async function runReplay(request: {
  session_id: string;
  slug: string;
  options: ReplayRunOptions;
  baseline_run_id?: string | null;
}): Promise<ReplayReport> {
  return invoke('replay_run', { request });
}

export async function runReplayAb(request: {
  left_session_id: string;
  right_session_id: string;
  slug: string;
  options: ReplayRunOptions;
}): Promise<ReplayAbReport> {
  return invoke('replay_run_ab', { request });
}

/** The last run of a set, whichever kind it was. */
export interface LastReport {
  report?: ReplayReport | null;
  ab?: ReplayAbReport | null;
}

export async function loadLastReport(slug: string): Promise<LastReport> {
  return invoke('replay_last_report', { slug });
}

export async function cancelReplayRun(): Promise<void> {
  return invoke('replay_cancel_run');
}

export async function listReplayRuns(slug: string): Promise<RunMeta[]> {
  return invoke('replay_list_runs', { slug });
}

/** Promotes what a run observed to the set's expectation. */
export async function acceptReplayRun(request: {
  slug: string;
  run_id: string;
  entry_ids?: string[] | null;
}): Promise<ReplaySet> {
  return invoke('replay_accept_run', { request });
}

export async function loadReplayCapture(runId: string, entryId: string): Promise<QueryResult> {
  return invoke('replay_load_capture', { runId, entryId });
}

/** Verdicts worth acting on, in the order the report lists them. */
export const FAILING_VERDICTS: ReplayVerdict[] = [
  'broken',
  'row_count_diff',
  'digest_diff',
  'slower',
];

export function hasRegressions(summary: ReplaySummary): boolean {
  return summary.broken + summary.row_count_diff + summary.digest_diff + summary.slower > 0;
}

/** Entries whose result was actually held against a reference. */
export function comparedCount(summary: ReplaySummary): number {
  return summary.matched + summary.row_count_diff + summary.digest_diff + summary.slower;
}

export function summarizeVerdicts(results: ReplayEntryResult[]): ReplaySummary {
  const summary: ReplaySummary = {
    total: results.length,
    matched: 0,
    broken: 0,
    row_count_diff: 0,
    digest_diff: 0,
    slower: 0,
    not_compared: 0,
    skipped: 0,
  };
  for (const result of results) {
    if (result.verdict === 'match') summary.matched += 1;
    else if (result.verdict === 'broken') summary.broken += 1;
    else if (result.verdict === 'row_count_diff') summary.row_count_diff += 1;
    else if (result.verdict === 'digest_diff') summary.digest_diff += 1;
    else if (result.verdict === 'slower') summary.slower += 1;
    else if (result.verdict === 'not_compared') summary.not_compared += 1;
    else summary.skipped += 1;
  }
  return summary;
}

/** Only a verdict that names a result difference has a diff worth opening. */
export function hasResultDiff(verdict: ReplayVerdict): boolean {
  return verdict === 'row_count_diff' || verdict === 'digest_diff';
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
