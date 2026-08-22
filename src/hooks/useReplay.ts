// SPDX-License-Identifier: BUSL-1.1

import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { notify } from '@/lib/notify';
import {
  type CaptureMode,
  cancelRecording,
  cancelReplayRun,
  DEFAULT_RUN_OPTIONS,
  deleteReplaySet,
  discardRecorded,
  getRecordedPreviews,
  getRecordingStatus,
  type LastReport,
  listReplayRuns,
  listReplaySets,
  loadLastReport,
  loadReplaySet,
  REPLAY_PROGRESS_EVENT,
  type RecordedPreview,
  type RecordingStatus,
  type ReplayAbReport,
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
} from '@/lib/replay';
import { getSecretPolicy } from '@/lib/replayPreferences';
import { listSessions, type SessionListItem } from '@/lib/tauri';
import { listen, type UnlistenFn } from '@/lib/transport';

/** Refresh cadence of the live recording counter. */
const RECORDING_POLL_MS = 1500;

export function useReplay(sessionId: string | null) {
  const { t } = useTranslation();

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

  const refreshSets = useCallback(async () => {
    setSetsLoading(true);
    try {
      setSets(await listReplaySets());
    } catch (err) {
      notify.error(t('replay.errors.listSets'), String(err));
    } finally {
      setSetsLoading(false);
    }
  }, [t]);

  const refreshRuns = useCallback(async (slug: string) => {
    try {
      setRuns(await listReplayRuns(slug));
    } catch {
      setRuns([]);
    }
  }, []);

  const selectSet = useCallback(
    async (slug: string) => {
      try {
        const set = await loadReplaySet(slug);
        setActiveSet(set);
        setActiveSlug(slug);
        // The last run is on disk: reopening the tab shows it again, as the
        // kind of comparison it actually was.
        const last = await loadLastReport(slug).catch(() => ({}) as LastReport);
        setReport(last.report ?? null);
        setAbReport(last.ab ?? null);
        await refreshRuns(slug);
      } catch (err) {
        notify.error(t('replay.errors.loadSet'), String(err));
      }
    },
    [refreshRuns, t]
  );

  const refreshRecording = useCallback(async () => {
    try {
      const status = await getRecordingStatus();
      setRecording(status);
      setPreviews(status ? await getRecordedPreviews() : []);
    } catch {
      setRecording(null);
      setPreviews([]);
    }
  }, []);

  useEffect(() => {
    void refreshSets();
    void refreshRecording();
    void listSessions()
      .then(setSessions)
      .catch(() => setSessions([]));
  }, [refreshSets, refreshRecording]);

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
      setProgress(event.payload);
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
  }, []);

  const beginRecording = useCallback(
    async (options: {
      name: string;
      ignoredColumns: string[];
      captureMode: CaptureMode;
      allowProductionCapture: boolean;
    }) => {
      if (!sessionId) {
        notify.error(t('replay.errors.noConnection'));
        return;
      }
      try {
        const status = await startRecording({
          session_id: sessionId,
          name: options.name,
          ignored_columns: options.ignoredColumns,
          capture_mode: options.captureMode,
          allow_production_capture: options.allowProductionCapture,
          // Read at start: the set is governed by the policy in force when it
          // was recorded, not by whatever the setting says later.
          secret_policy: getSecretPolicy(),
        });
        setRecording(status);
        setPreviews([]);
      } catch (err) {
        notify.error(t('replay.errors.startRecording'), String(err));
      }
    },
    [sessionId, t]
  );

  const endRecording = useCallback(async () => {
    try {
      const summary = await stopRecording();
      setRecording(null);
      setPreviews([]);
      await refreshSets();
      await selectSet(summary.slug);
      notify.success(t('replay.recordingSaved', { name: summary.name }));
      return summary;
    } catch (err) {
      notify.error(t('replay.errors.stopRecording'), String(err));
      return null;
    }
  }, [refreshSets, selectSet, t]);

  const abortRecording = useCallback(async () => {
    await cancelRecording();
    setRecording(null);
    setPreviews([]);
  }, []);

  const dropRecorded = useCallback(
    async (index: number) => {
      try {
        await discardRecorded(index);
        await refreshRecording();
      } catch (err) {
        notify.error(t('replay.errors.discard'), String(err));
      }
    },
    [refreshRecording, t]
  );

  const replay = useCallback(
    async (options: ReplayRunOptions = DEFAULT_RUN_OPTIONS, baselineRunId?: string) => {
      if (!sessionId) {
        notify.error(t('replay.errors.noConnection'));
        return;
      }
      if (!activeSlug) return;

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
        setReport(result);
        await refreshRuns(activeSlug);
      } catch (err) {
        notify.error(t('replay.errors.run'), String(err));
      } finally {
        setRunning(false);
        setProgress(null);
      }
    },
    [activeSlug, refreshRuns, sessionId, t]
  );

  const replayAb = useCallback(
    async (rightSessionId: string, options: ReplayRunOptions = DEFAULT_RUN_OPTIONS) => {
      if (!sessionId) {
        notify.error(t('replay.errors.noConnection'));
        return;
      }
      if (!activeSlug) return;

      setRunning(true);
      setProgress(null);
      setReport(null);
      try {
        setAbReport(
          await runReplayAb({
            left_session_id: sessionId,
            right_session_id: rightSessionId,
            slug: activeSlug,
            options,
          })
        );
        await refreshRuns(activeSlug);
      } catch (err) {
        notify.error(t('replay.errors.run'), String(err));
      } finally {
        setRunning(false);
        setProgress(null);
      }
    },
    [activeSlug, refreshRuns, sessionId, t]
  );

  const abortReplay = useCallback(async () => {
    await cancelReplayRun();
  }, []);

  const removeSet = useCallback(
    async (slug: string) => {
      try {
        await deleteReplaySet(slug);
        if (activeSlug === slug) {
          setActiveSet(null);
          setActiveSlug(null);
          setReport(null);
          setRuns([]);
        }
        await refreshSets();
      } catch (err) {
        notify.error(t('replay.errors.deleteSet'), String(err));
      }
    },
    [activeSlug, refreshSets, t]
  );

  const updateIgnoredColumns = useCallback(
    async (columns: string[]) => {
      if (!activeSlug) return;
      try {
        setActiveSet(await setIgnoredColumns(activeSlug, columns));
      } catch (err) {
        notify.error(t('replay.errors.ignoredColumns'), String(err));
      }
    },
    [activeSlug, t]
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
    replay,
    replayAb,
    abortReplay,
    removeSet,
    updateIgnoredColumns,
  };
}
