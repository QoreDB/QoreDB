// SPDX-License-Identifier: BUSL-1.1

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { notify } from '@/lib/notify';
import {
  acceptReplayRun,
  type CaptureMode,
  cancelRecording,
  cancelReplayRun,
  DEFAULT_RUN_OPTIONS,
  deleteReplaySet,
  discardRecorded,
  discardRecordedMutations,
  getRecordedPreviews,
  getRecordingStatus,
  listReplayRuns,
  listReplaySets,
  loadLastReport,
  loadReplaySet,
  RECORDING_CHANGED_EVENT,
  REPLAY_PROGRESS_EVENT,
  type RecordedPreview,
  type RecordingStatus,
  type ReplayAbReport,
  type ReplayEntryResult,
  type ReplayProgress,
  type ReplayReport,
  type ReplayRunOptions,
  type ReplaySet,
  type ReplaySetSummary,
  type RunMeta,
  runReplay,
  runReplayAb,
  setIgnoredColumns,
  startRecording,
  stopRecording,
  summarizeVerdicts,
} from '@/lib/replay';
import { getSecretPolicy } from '@/lib/replayPreferences';
import { useWorkspaceStore } from '@/lib/stores/workspaceStore';
import { listSessions, type SessionListItem } from '@/lib/tauri';
import { listen, type UnlistenFn } from '@/lib/transport';

/** Refresh cadence of the live recording counter. */
const RECORDING_POLL_MS = 1500;

export function useReplay(sessionId: string | null) {
  const { t } = useTranslation();
  const projectId = useWorkspaceStore(state => state.projectId);
  const context = useMemo(() => ({ sessionId, projectId }), [sessionId, projectId]);
  const contextRef = useRef<typeof context | null>(context);
  contextRef.current = context;
  const selectionRef = useRef<{ slug: string } | null>(null);
  const runRef = useRef<object | null>(null);
  const reportReadRef = useRef<object | null>(null);
  const recordingReadRef = useRef<object | null>(null);
  const isCurrent = useCallback(
    (selection?: object | null) =>
      contextRef.current === context &&
      (selection === undefined || selectionRef.current === selection),
    [context]
  );

  const [sets, setSets] = useState<ReplaySetSummary[]>([]);
  const [setsLoading, setSetsLoading] = useState(false);
  const [activeSet, setActiveSet] = useState<ReplaySet | null>(null);
  const [activeSlug, setActiveSlug] = useState<string | null>(null);
  const [runs, setRuns] = useState<RunMeta[]>([]);

  const [recording, setRecording] = useState<RecordingStatus | null>(null);
  const [previews, setPreviews] = useState<RecordedPreview[]>([]);

  const [report, setReport] = useState<ReplayReport | null>(null);
  const [abReport, setAbReport] = useState<ReplayAbReport | null>(null);
  const [sessions, setSessions] = useState<SessionListItem[]>([]);
  const [progress, setProgress] = useState<ReplayProgress | null>(null);
  const [running, setRunning] = useState(false);

  useEffect(() => {
    contextRef.current = context;
    selectionRef.current = null;
    runRef.current = null;
    reportReadRef.current = null;
    recordingReadRef.current = null;
    setActiveSet(null);
    setActiveSlug(null);
    setSets([]);
    setRuns([]);
    setReport(null);
    setAbReport(null);
    setRecording(null);
    setPreviews([]);
    setSessions([]);
    setProgress(null);
    setRunning(false);
    return () => {
      contextRef.current = null;
    };
  }, [context]);

  const refreshSets = useCallback(async () => {
    if (!isCurrent()) return;
    setSetsLoading(true);
    try {
      const next = await listReplaySets();
      if (isCurrent()) setSets(next);
    } catch (err) {
      if (isCurrent()) notify.error(t('replay.errors.listSets'), String(err));
    } finally {
      if (isCurrent()) setSetsLoading(false);
    }
  }, [isCurrent, t]);

  const refreshRuns = useCallback(
    async (slug: string, selection = selectionRef.current) => {
      if (!isCurrent(selection)) return;
      try {
        const next = await listReplayRuns(slug);
        if (isCurrent(selection)) setRuns(next);
      } catch {
        if (isCurrent(selection)) setRuns([]);
      }
    },
    [isCurrent]
  );

  const selectSet = useCallback(
    async (slug: string) => {
      if (!isCurrent()) return;
      const selection = { slug };
      selectionRef.current = selection;
      reportReadRef.current = selection;
      setActiveSet(null);
      setActiveSlug(null);
      setReport(null);
      setAbReport(null);
      setRuns([]);
      try {
        const set = await loadReplaySet(slug);
        if (!isCurrent(selection)) return;
        setActiveSet(set);
        setActiveSlug(slug);
        // The last run is on disk: reopening the tab shows it again, as the
        // kind of comparison it actually was.
        const last = await loadLastReport(slug);
        if (!isCurrent(selection) || reportReadRef.current !== selection) return;
        setReport(last.report ?? null);
        setAbReport(last.ab ?? null);
        await refreshRuns(slug, selection);
      } catch (err) {
        if (isCurrent(selection) && reportReadRef.current === selection) {
          notify.error(t('replay.errors.loadSet'), String(err));
        }
      }
    },
    [isCurrent, refreshRuns, t]
  );

  const refreshRecording = useCallback(async () => {
    if (!isCurrent()) return;
    const read = {};
    recordingReadRef.current = read;
    try {
      const status = await getRecordingStatus(projectId);
      if (!isCurrent() || recordingReadRef.current !== read) return;
      const next = status ? await getRecordedPreviews({ projectId, runId: status.run_id }) : [];
      if (!isCurrent() || recordingReadRef.current !== read) return;
      setRecording(status);
      setPreviews(next);
    } catch {
      if (isCurrent() && recordingReadRef.current === read) {
        setRecording(null);
        setPreviews([]);
      }
    }
  }, [isCurrent, projectId]);

  useEffect(() => {
    void refreshSets();
    void refreshRecording();
    void listSessions()
      .then(next => {
        if (isCurrent()) setSessions(next);
      })
      .catch(() => {
        if (isCurrent()) setSessions([]);
      });
  }, [isCurrent, refreshSets, refreshRecording]);

  useEffect(() => {
    const onChanged = () => {
      void refreshRecording();
      void refreshSets();
    };
    window.addEventListener(RECORDING_CHANGED_EVENT, onChanged);
    return () => window.removeEventListener(RECORDING_CHANGED_EVENT, onChanged);
  }, [refreshRecording, refreshSets]);

  // The recorder lives in the backend and fills up as the user runs queries in
  // other tabs, so the counter is polled rather than pushed.
  useEffect(() => {
    if (!recording) return;
    const timer = window.setInterval(() => void refreshRecording(), RECORDING_POLL_MS);
    return () => window.clearInterval(timer);
  }, [recording, refreshRecording]);

  const unlistenRef = useRef<UnlistenFn | null>(null);
  useEffect(() => {
    let cancelled = false;
    void listen<ReplayProgress>(REPLAY_PROGRESS_EVENT, event => {
      if (!cancelled && isCurrent() && runRef.current) setProgress(event.payload);
    }).then(unlisten => {
      if (cancelled) {
        unlisten();
        return;
      }
      unlistenRef.current = unlisten;
    });
    return () => {
      cancelled = true;
      unlistenRef.current?.();
      unlistenRef.current = null;
    };
  }, [isCurrent]);

  const beginRecording = useCallback(
    async (options: {
      name: string;
      ignoredColumns: string[];
      recordMutations: boolean;
      captureMode: CaptureMode;
      allowProductionCapture: boolean;
    }) => {
      if (!isCurrent()) return;
      if (!sessionId) {
        notify.error(t('replay.errors.noConnection'));
        return;
      }
      try {
        recordingReadRef.current = null;
        const status = await startRecording({
          project_id: projectId,
          session_id: sessionId,
          name: options.name,
          ignored_columns: options.ignoredColumns,
          record_mutations: options.recordMutations,
          capture_mode: options.captureMode,
          allow_production_capture: options.allowProductionCapture,
          // Read at start: the set is governed by the policy in force when it
          // was recorded, not by whatever the setting says later.
          secret_policy: getSecretPolicy(),
        });
        if (!isCurrent()) return;
        setRecording(status);
        setPreviews([]);
      } catch (err) {
        if (isCurrent()) notify.error(t('replay.errors.startRecording'), String(err));
      }
    },
    [isCurrent, projectId, sessionId, t]
  );

  const endRecording = useCallback(async () => {
    if (!isCurrent() || !recording) return null;
    recordingReadRef.current = null;
    try {
      const summary = await stopRecording({ projectId, runId: recording.run_id });
      if (!isCurrent()) return null;
      setRecording(null);
      setPreviews([]);
      await refreshSets();
      if (!isCurrent()) return null;
      await selectSet(summary.slug);
      if (!isCurrent()) return null;
      notify.success(t('replay.recordingSaved', { name: summary.name }));
      return summary;
    } catch (err) {
      if (isCurrent()) notify.error(t('replay.errors.stopRecording'), String(err));
      return null;
    }
  }, [isCurrent, projectId, recording, refreshSets, selectSet, t]);

  const abortRecording = useCallback(async () => {
    if (!isCurrent() || !recording) return;
    recordingReadRef.current = null;
    try {
      await cancelRecording({ projectId, runId: recording.run_id });
    } catch (err) {
      if (isCurrent()) {
        notify.error(t('replay.errors.discard'), String(err));
        await refreshRecording();
      }
      return;
    }
    if (!isCurrent() || !recording) return;
    setRecording(null);
    setPreviews([]);
  }, [isCurrent, projectId, recording, refreshRecording, t]);

  const dropMutations = useCallback(async () => {
    if (!isCurrent() || !recording) return;
    try {
      recordingReadRef.current = null;
      await discardRecordedMutations({ projectId, runId: recording.run_id });
      if (!isCurrent() || !recording) return;
      await refreshRecording();
    } catch (err) {
      if (isCurrent()) {
        notify.error(t('replay.errors.discard'), String(err));
        await refreshRecording();
      }
    }
  }, [isCurrent, projectId, recording, refreshRecording, t]);

  const dropRecorded = useCallback(
    async (index: number) => {
      if (!isCurrent() || !recording) return;
      try {
        recordingReadRef.current = null;
        await discardRecorded({ projectId, runId: recording.run_id }, index);
        if (!isCurrent() || !recording) return;
        await refreshRecording();
      } catch (err) {
        if (isCurrent()) {
          notify.error(t('replay.errors.discard'), String(err));
          await refreshRecording();
        }
      }
    },
    [isCurrent, projectId, recording, refreshRecording, t]
  );

  const replay = useCallback(
    async (options: ReplayRunOptions = DEFAULT_RUN_OPTIONS, baselineRunId?: string) => {
      if (!isCurrent() || runRef.current) return;
      if (!sessionId) {
        notify.error(t('replay.errors.noConnection'));
        return;
      }
      if (!activeSlug || !isCurrent() || selectionRef.current?.slug !== activeSlug) return;
      const selection = selectionRef.current;
      const operation = {};
      runRef.current = operation;
      reportReadRef.current = null;
      setRunning(true);
      setProgress(null);
      setAbReport(null);
      try {
        const result = await runReplay({
          session_id: sessionId,
          slug: activeSlug,
          options,
          baseline_run_id: baselineRunId ?? null,
        });
        if (!isCurrent(selection)) return;
        setReport(result);
        await refreshRuns(activeSlug, selection);
      } catch (err) {
        if (isCurrent(selection)) notify.error(t('replay.errors.run'), String(err));
      } finally {
        if (isCurrent() && runRef.current === operation) {
          runRef.current = null;
          setRunning(false);
          setProgress(null);
        }
      }
    },
    [activeSlug, isCurrent, refreshRuns, sessionId, t]
  );

  const replayAb = useCallback(
    async (rightSessionId: string, options: ReplayRunOptions = DEFAULT_RUN_OPTIONS) => {
      if (!isCurrent() || runRef.current) return;
      if (!sessionId) {
        notify.error(t('replay.errors.noConnection'));
        return;
      }
      if (!activeSlug || !isCurrent() || selectionRef.current?.slug !== activeSlug) return;
      const selection = selectionRef.current;
      const operation = {};
      runRef.current = operation;
      reportReadRef.current = null;
      setRunning(true);
      setProgress(null);
      setReport(null);
      try {
        const result = await runReplayAb({
          left_session_id: sessionId,
          right_session_id: rightSessionId,
          slug: activeSlug,
          options,
        });
        if (!isCurrent(selection)) return;
        setAbReport(result);
        await refreshRuns(activeSlug, selection);
      } catch (err) {
        if (isCurrent(selection)) notify.error(t('replay.errors.run'), String(err));
      } finally {
        if (isCurrent() && runRef.current === operation) {
          runRef.current = null;
          setRunning(false);
          setProgress(null);
        }
      }
    },
    [activeSlug, isCurrent, refreshRuns, sessionId, t]
  );

  const abortReplay = useCallback(async () => {
    if (!isCurrent() || !runRef.current) return;
    await cancelReplayRun();
  }, [isCurrent]);

  const removeSet = useCallback(
    async (slug: string) => {
      if (!isCurrent()) return;
      const selection = selectionRef.current;
      try {
        await deleteReplaySet(slug);
        if (!isCurrent()) return;
        if (isCurrent(selection) && selectionRef.current?.slug === slug) {
          selectionRef.current = null;
          setActiveSet(null);
          setActiveSlug(null);
          setReport(null);
          setAbReport(null);
          setRuns([]);
        }
        await refreshSets();
      } catch (err) {
        if (isCurrent()) notify.error(t('replay.errors.deleteSet'), String(err));
      }
    },
    [isCurrent, refreshSets, t]
  );

  const acceptRun = useCallback(
    async (runId: string, entryIds?: string[]) => {
      if (!activeSlug || !isCurrent() || selectionRef.current?.slug !== activeSlug) return;
      const selection = selectionRef.current;
      try {
        const next = await acceptReplayRun({
          slug: activeSlug,
          run_id: runId,
          entry_ids: entryIds ?? null,
        });
        if (!isCurrent(selection)) return;
        setActiveSet(next);
        // The accepted entries are now their own expectation, so the report on
        // screen would otherwise keep flagging what the user just accepted.
        setReport(current => {
          if (!current || current.run.run_id !== runId) return current;
          const targeted = entryIds ? new Set(entryIds) : null;
          const results = current.results.map<ReplayEntryResult>(result => {
            if (result.verdict === 'skipped') return result;
            if (targeted && !targeted.has(result.entry_id)) return result;
            return {
              ...result,
              verdict: 'match',
              expected_row_count: result.row_count,
              expected_digest: result.digest,
              expected_execution_time_ms: result.execution_time_ms,
            };
          });
          return { ...current, results, summary: summarizeVerdicts(results) };
        });
        notify.success(
          entryIds
            ? t('replay.referenceAcceptedEntries', { count: entryIds.length })
            : t('replay.referenceAcceptedAll')
        );
        await refreshRuns(activeSlug, selection);
      } catch (err) {
        if (isCurrent(selection)) notify.error(t('replay.errors.acceptRun'), String(err));
      }
    },
    [activeSlug, isCurrent, refreshRuns, t]
  );

  const updateIgnoredColumns = useCallback(
    async (columns: string[]) => {
      if (!activeSlug || !isCurrent() || selectionRef.current?.slug !== activeSlug) return;
      const selection = selectionRef.current;
      try {
        const next = await setIgnoredColumns(activeSlug, columns);
        if (isCurrent(selection)) setActiveSet(next);
      } catch (err) {
        if (isCurrent(selection)) notify.error(t('replay.errors.ignoredColumns'), String(err));
      }
    },
    [activeSlug, isCurrent, t]
  );

  return {
    sets,
    setsLoading,
    activeSet,
    activeSlug,
    runs,
    recording,
    previews,
    report,
    abReport,
    sessions,
    progress,
    running,
    selectSet,
    beginRecording,
    endRecording,
    abortRecording,
    dropRecorded,
    dropMutations,
    acceptRun,
    replay,
    replayAb,
    abortReplay,
    removeSet,
    updateIgnoredColumns,
  };
}
