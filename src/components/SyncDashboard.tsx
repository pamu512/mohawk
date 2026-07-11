import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type FC,
} from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  ApiService,
  PendingCourseSummary,
  SyncDashboardStatus,
  SyncLogLine,
  isDesktopShell,
  TAURI_IPC_ERROR,
} from '../services/api';

const STATUS_REFRESH_MS = 30_000;

function formatCountdown(totalSeconds: number): string {
  if (totalSeconds <= 0) return 'DUE NOW — AWAITING CYCLE';
  const days = Math.floor(totalSeconds / 86_400);
  const hours = Math.floor((totalSeconds % 86_400) / 3_600);
  const minutes = Math.floor((totalSeconds % 3_600) / 60);
  const seconds = totalSeconds % 60;
  return `${days}d ${String(hours).padStart(2, '0')}h ${String(minutes).padStart(2, '0')}m ${String(seconds).padStart(2, '0')}s`;
}

function formatTimestamp(iso: string | null | undefined): string {
  if (!iso) return 'NEVER';
  try {
    return new Date(iso).toLocaleString(undefined, {
      dateStyle: 'medium',
      timeStyle: 'short',
    });
  } catch {
    return iso;
  }
}

function logLineClass(text: string): string {
  if (text.includes('[ERROR]')) return 'text-rose-400';
  if (text.includes('[WARN]')) return 'text-amber-400';
  if (text.includes('[COMPLETE]')) return 'text-emerald-400';
  if (text.includes('[REVIEW]')) return 'text-violet-300';
  if (text.includes('[SYNTHESIZING]')) return 'text-violet-400';
  if (text.includes('[FETCHING]')) return 'text-cyan-400';
  return 'text-slate-400';
}

export const SyncDashboard: FC = () => {
  const [status, setStatus] = useState<SyncDashboardStatus | null>(null);
  const [logs, setLogs] = useState<SyncLogLine[]>([]);
  const [pending, setPending] = useState<PendingCourseSummary[]>([]);
  const [countdownSec, setCountdownSec] = useState(0);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [forceError, setForceError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [forcing, setForcing] = useState(false);
  const [actingId, setActingId] = useState<string | null>(null);
  const [inDesktopShell, setInDesktopShell] = useState(false);
  const consoleRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setInDesktopShell(isDesktopShell());
  }, []);

  const refresh = useCallback(async () => {
    if (!inDesktopShell) {
      setLoadError(TAURI_IPC_ERROR);
      return;
    }
    try {
      const next = await ApiService.getSyncDashboardStatus();
      setStatus(next);
      setLogs(next.logs);
      setPending(next.pending_courses);
      setCountdownSec(next.seconds_until_next_sync);
      setLoadError(null);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setLoadError(message);
    }
  }, [inDesktopShell]);

  useEffect(() => {
    if (!inDesktopShell) return;
    refresh();
    const timer = setInterval(refresh, STATUS_REFRESH_MS);
    return () => clearInterval(timer);
  }, [refresh, inDesktopShell]);

  useEffect(() => {
    if (!inDesktopShell) return;

    const unlisteners: UnlistenFn[] = [];

    void (async () => {
      unlisteners.push(
        await listen<SyncLogLine>('sync-log-line', (event) => {
          setLogs((prev) => [...prev.slice(-149), event.payload]);
        }),
      );
      unlisteners.push(
        await listen('sync-status-changed', () => {
          void refresh();
        }),
      );
      unlisteners.push(
        await listen<PendingCourseSummary[]>('pending-courses-changed', (event) => {
          setPending(event.payload);
        }),
      );
    })();

    return () => {
      unlisteners.forEach((fn) => fn());
    };
  }, [inDesktopShell, refresh]);

  useEffect(() => {
    const timer = setInterval(() => {
      setCountdownSec((sec) => Math.max(0, sec - 1));
    }, 1_000);
    return () => clearInterval(timer);
  }, []);

  useEffect(() => {
    const el = consoleRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [logs.length]);

  const handleForceSync = async () => {
    if (!inDesktopShell) {
      setForceError(TAURI_IPC_ERROR);
      return;
    }
    setForceError(null);
    setForcing(true);
    try {
      await ApiService.forceSyncCurriculum();
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setForceError(message);
    } finally {
      setForcing(false);
    }
  };

  const handleAccept = async (id: string) => {
    setActionError(null);
    setActingId(id);
    try {
      await ApiService.acceptPendingCourse(id);
      await refresh();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setActingId(null);
    }
  };

  const handleReject = async (id: string) => {
    setActionError(null);
    setActingId(id);
    try {
      await ApiService.rejectPendingCourse(id);
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setActingId(null);
    }
  };

  const inProgress = status?.sync_in_progress ?? false;
  const countdown = formatCountdown(countdownSec);
  const dueNow = countdownSec <= 0 || (status?.sync_is_due ?? false);
  const ollamaOnline = status?.ollama.online ?? false;

  return (
    <div className="flex h-full w-full flex-col overflow-hidden bg-slate-950 font-mono text-slate-100">
      <header className="border-b border-slate-800 px-6 py-4">
        {!inDesktopShell && (
          <div className="mb-3 rounded border border-amber-800/60 bg-amber-950/30 px-3 py-2 text-[11px] text-amber-300">
            Browser-only mode — IPC bridge missing. Run{' '}
            <code className="text-amber-200">npm run dev</code> from the repo root and use the{' '}
            <strong className="font-bold text-amber-200">Mohawk</strong> app window (not a Chrome/Safari
            tab on localhost:1420).
          </div>
        )}
        <div className="flex items-start justify-between gap-4">
          <div>
            <div className="text-xs font-bold uppercase tracking-wider text-amber-400">
              // THREAT INTEL DESK
            </div>
            <p className="mt-1 max-w-3xl text-[11px] leading-relaxed text-slate-500">
              Monitor the biweekly curriculum curation pipeline — RSS ingestion, prose chunking, and
              local Ollama synthesis into graph-linked study material.
            </p>
          </div>
          <div className="shrink-0 rounded border border-slate-800 bg-slate-900/60 px-3 py-2 text-[10px]">
            <div className="text-slate-600">OLLAMA</div>
            <div className={ollamaOnline ? 'font-bold text-emerald-400' : 'font-bold text-rose-400'}>
              {ollamaOnline ? 'ONLINE' : 'OFFLINE'}
            </div>
            {status?.ollama && (
              <div className="mt-0.5 text-slate-500">
                {status.ollama.model_count} model(s) · {status.ollama.default_model}
              </div>
            )}
          </div>
        </div>
      </header>

      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-6 lg:flex-row">
        <div className="flex w-full flex-col gap-4 lg:w-2/5">
          <section className="rounded-lg border border-slate-800 bg-slate-900/40 p-4">
            <div className="mb-3 text-[10px] font-semibold uppercase tracking-wider text-slate-500">
              // NEXT AUTOMATED SYNC
            </div>
            <div
              className={`text-2xl font-black tracking-tight ${
                dueNow ? 'animate-pulse text-rose-400' : 'text-amber-300'
              }`}
            >
              {countdown}
            </div>
            <div className="mt-3 grid grid-cols-2 gap-2 text-[10px] text-slate-500">
              <div>
                <span className="text-slate-600">LAST_SYNC:</span>{' '}
                <span className="text-slate-300">{formatTimestamp(status?.last_sync_at)}</span>
              </div>
              <div>
                <span className="text-slate-600">NEXT_WINDOW:</span>{' '}
                <span className="text-slate-300">{formatTimestamp(status?.next_sync_at)}</span>
              </div>
              <div>
                <span className="text-slate-600">INTERVAL:</span>{' '}
                <span className="text-slate-300">
                  {status?.sync_interval_days ?? 14}d biweekly
                </span>
              </div>
              <div>
                <span className="text-slate-600">WORKER:</span>{' '}
                <span className={inProgress ? 'text-cyan-400' : 'text-emerald-400'}>
                  {inProgress ? 'ACTIVE' : 'IDLE'}
                </span>
              </div>
            </div>
            {status?.last_report && (
              <div className="mt-3 rounded border border-slate-800 bg-black/30 px-3 py-2 text-[10px] text-slate-500">
                LAST_RUN — {status.last_report.chunks_produced} chunks /{' '}
                {status.last_report.feeds_succeeded}/{status.last_report.feeds_attempted} feeds /{' '}
                {status.last_report.error_count} warn(s)
              </div>
            )}
          </section>

          <section className="rounded-lg border border-violet-900/40 bg-violet-950/10 p-4">
            <div className="mb-3 flex items-center justify-between">
              <span className="text-[10px] font-semibold uppercase tracking-wider text-slate-500">
                // PENDING COURSE REVIEW
              </span>
              <span className="text-[9px] text-slate-600">{pending.length} queued</span>
            </div>
            {actionError && (
              <p className="mb-2 text-[10px] text-rose-400">{actionError}</p>
            )}
            <ul className="max-h-48 space-y-2 overflow-y-auto">
              {pending.length === 0 && (
                <li className="text-[11px] text-slate-600">No staged courses — run a sync to extract.</li>
              )}
              {pending.map((course) => (
                <li
                  key={course.id}
                  className="rounded border border-slate-800 bg-slate-950/60 px-3 py-2.5"
                >
                  <div className="text-xs font-bold text-violet-200">{course.course_title}</div>
                  <div className="mt-0.5 text-[10px] text-slate-500">
                    [{course.category}] {course.node_count} nodes · {course.edge_count} edges ·{' '}
                    {course.card_count} cards
                  </div>
                  <div className="truncate text-[9px] text-slate-600">
                    {course.source_title} · chunk {course.chunk_index}
                  </div>
                  <div className="mt-2 flex gap-2">
                    <button
                      type="button"
                      disabled={actingId === course.id}
                      onClick={() => handleAccept(course.id)}
                      className="rounded border border-emerald-800 bg-emerald-950/40 px-2 py-1 text-[10px] font-bold text-emerald-300 hover:bg-emerald-900/50 disabled:opacity-40"
                    >
                      ACCEPT
                    </button>
                    <button
                      type="button"
                      disabled={actingId === course.id}
                      onClick={() => handleReject(course.id)}
                      className="rounded border border-slate-700 bg-slate-900/40 px-2 py-1 text-[10px] font-bold text-slate-400 hover:bg-slate-800/50 disabled:opacity-40"
                    >
                      REJECT
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          </section>

          <section className="min-h-0 flex-1 rounded-lg border border-slate-800 bg-slate-900/40 p-4">
            <div className="mb-3 text-[10px] font-semibold uppercase tracking-wider text-slate-500">
              // INGESTION SOURCES
            </div>
            <ul className="space-y-2 overflow-y-auto">
              {(status?.sources ?? []).map((source) => (
                <li
                  key={source.id}
                  className="flex items-start gap-3 rounded border border-slate-800 bg-slate-950/60 px-3 py-2.5"
                >
                  <span
                    className={`mt-0.5 h-2 w-2 shrink-0 rounded-full ${
                      inProgress ? 'animate-pulse bg-cyan-400' : 'bg-emerald-500'
                    }`}
                    aria-hidden
                  />
                  <div className="min-w-0 flex-1">
                    <div className="text-xs font-bold text-slate-200">{source.label}</div>
                    <div className="truncate text-[10px] text-slate-600">{source.url}</div>
                  </div>
                </li>
              ))}
              {!status?.sources.length && (
                <li className="text-[11px] text-slate-600">Loading source matrix...</li>
              )}
            </ul>
          </section>

          <section className="rounded-lg border border-rose-900/40 bg-rose-950/10 p-4">
            <button
              type="button"
              onClick={handleForceSync}
              disabled={forcing || inProgress || !ollamaOnline}
              className="w-full rounded border-2 border-rose-700 bg-rose-900/60 py-4 text-xs font-black tracking-widest text-rose-100 shadow-lg shadow-rose-950/50 transition hover:border-rose-500 hover:bg-rose-800/80 disabled:cursor-not-allowed disabled:opacity-40"
            >
              {forcing || inProgress
                ? 'SYNC PIPELINE ENGAGED...'
                : 'FORCE SYNC CRITICAL OUTBREAK VECTOR'}
            </button>
            {!ollamaOnline && (
              <p className="mt-2 text-[10px] text-amber-400">Start Ollama before forcing sync.</p>
            )}
            {forceError && <p className="mt-2 text-[10px] text-rose-400">{forceError}</p>}
          </section>
        </div>

        <section className="flex min-h-64 flex-1 flex-col rounded-lg border border-slate-800 bg-black/50 lg:min-h-0">
          <div className="flex items-center justify-between border-b border-slate-800 px-4 py-2">
            <span className="text-[10px] font-semibold uppercase tracking-wider text-slate-500">
              // BACKGROUND WORKER CONSOLE
            </span>
            <span className="text-[9px] text-slate-600">{logs.length} line(s) · live events</span>
          </div>
          <div
            ref={consoleRef}
            className="min-h-0 flex-1 overflow-y-auto p-4 text-[11px] leading-relaxed"
          >
            {loadError && <div className="mb-2 text-rose-400">[IPC ERR] {loadError}</div>}
            {logs.map((line, i) => (
              <div key={`${line.timestamp}-${i}`} className={`mb-1 ${logLineClass(line.text)}`}>
                <span className="mr-2 text-slate-700">
                  {new Date(line.timestamp).toLocaleTimeString(undefined, {
                    hour: '2-digit',
                    minute: '2-digit',
                    second: '2-digit',
                  })}
                </span>
                {line.text}
              </div>
            ))}
            {inProgress && (
              <div className="mt-2 inline-block h-3 w-2 animate-pulse bg-cyan-500" aria-hidden />
            )}
            {!logs.length && !loadError && (
              <span className="text-slate-600">Awaiting worker telemetry...</span>
            )}
          </div>
        </section>
      </div>
    </div>
  );
};
