import { useEffect, useState } from 'react';
import { StudyCircuit } from './components/StudyCircuit';
import { RuleSandbox } from './components/RuleSandbox';
import { TopologyGraph } from './components/TopologyGraph';
import { IngestIntel } from './components/IngestIntel';
import { SyncDashboard } from './components/SyncDashboard';
import { ApiService, isDesktopShell } from './services/api';
import type { StudyTrack } from './lib/studyTracks';

type ActivePanel = 'TRIAGE' | 'SANDBOX' | 'TOPOLOGY' | 'INGEST' | 'DESK';

function App() {
  const [activePanel, setActivePanel] = useState<ActivePanel>('TRIAGE');
  const [studyTrack, setStudyTrack] = useState<StudyTrack>('ALL');
  const [ollamaOnline, setOllamaOnline] = useState<boolean | null>(null);

  useEffect(() => {
    if (!isDesktopShell()) {
      setOllamaOnline(null);
      return;
    }

    let cancelled = false;
    const poll = async () => {
      try {
        const status = await ApiService.getSyncDashboardStatus();
        if (!cancelled) setOllamaOnline(status.ollama.online);
      } catch {
        if (!cancelled) setOllamaOnline(false);
      }
    };

    void poll();
    const timer = setInterval(poll, 30_000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, []);

  return (
    <div className="flex h-screen w-screen bg-slate-950 text-slate-100 overflow-hidden select-none font-mono">
      {/* Left Global Command Deck Sidebar */}
      <div className="w-64 border-r border-slate-800 bg-slate-950 flex flex-col justify-between p-4">
        <div className="flex flex-col gap-6">
          {/* Logo Brand Segment */}
          <div className="flex flex-col border-b border-dashed border-slate-800 pb-4">
            <span className="text-sm font-black tracking-widest text-slate-200">MOHAWK // RISK</span>
            <span className="text-[9px] text-slate-500 mt-0.5 font-bold">LOCAL SECURE COGNITIVE CORE</span>
          </div>

          {/* Navigation Matrix Panel */}
          <nav className="flex flex-col gap-2">
            <button
              onClick={() => {
                setStudyTrack('ALL');
                setActivePanel('TRIAGE');
              }}
              className={`w-full text-left font-mono text-xs px-3 py-2.5 rounded transition font-medium border ${
                activePanel === 'TRIAGE'
                  ? 'bg-slate-900 text-amber-400 border-slate-700 shadow'
                  : 'text-slate-400 border-transparent hover:bg-slate-900/50 hover:text-slate-200'
              }`}
            >
              [⚡] SIGNAL TRIAGE CIRCUIT
            </button>
            <button
              onClick={() => setActivePanel('SANDBOX')}
              className={`w-full text-left font-mono text-xs px-3 py-2.5 rounded transition font-medium border ${
                activePanel === 'SANDBOX'
                  ? 'bg-slate-900 text-indigo-400 border-slate-700 shadow'
                  : 'text-slate-400 border-transparent hover:bg-slate-900/50 hover:text-slate-200'
              }`}
            >
              [⚙] TRANSACTION SANDBOX
            </button>
            <button
              onClick={() => setActivePanel('TOPOLOGY')}
              className={`w-full text-left font-mono text-xs px-3 py-2.5 rounded transition font-medium border ${
                activePanel === 'TOPOLOGY'
                  ? 'bg-slate-900 text-rose-400 border-slate-700 shadow'
                  : 'text-slate-400 border-transparent hover:bg-slate-900/50 hover:text-slate-200'
              }`}
            >
              [☩] VECTOR ATTACK TOPOLOGY
            </button>
            <button
              onClick={() => setActivePanel('INGEST')}
              className={`w-full text-left font-mono text-xs px-3 py-2.5 rounded transition font-medium border ${
                activePanel === 'INGEST'
                  ? 'bg-slate-900 text-teal-400 border-slate-700 shadow'
                  : 'text-slate-400 border-transparent hover:bg-slate-900/50 hover:text-slate-200'
              }`}
            >
              [🗂] INGEST RISK INTEL
            </button>
            <button
              onClick={() => setActivePanel('DESK')}
              className={`w-full text-left font-mono text-xs px-3 py-2.5 rounded transition font-medium border ${
                activePanel === 'DESK'
                  ? 'bg-slate-900 text-amber-400 border-slate-700 shadow'
                  : 'text-slate-400 border-transparent hover:bg-slate-900/50 hover:text-slate-200'
              }`}
            >
              [🗲] THREAT INTEL DESK
            </button>
          </nav>
        </div>

        {/* Global Local Node Infrastructure Footer Status */}
        <div className="border-t border-slate-900 pt-4 text-[10px] text-slate-600 flex flex-col gap-1">
          <div>DATABASE_STATUS: SECURE_SQLITE</div>
          <div>FEEDS: HTTPS_LIVE</div>
          <div>
            OLLAMA:{' '}
            {ollamaOnline === null
              ? 'UNKNOWN'
              : ollamaOnline
                ? 'ONLINE'
                : 'OFFLINE'}
          </div>
        </div>
      </div>

      {/* Main Operations Terminal Window Panel */}
      <main className="flex-1 h-full bg-slate-950 overflow-hidden relative">
        <div className={activePanel === 'TRIAGE' ? 'h-full' : 'hidden'}>
          <StudyCircuit track={studyTrack} />
        </div>
        {activePanel === 'SANDBOX' && <RuleSandbox />}
        {activePanel === 'TOPOLOGY' && (
          <TopologyGraph onNavigateToTriage={() => setActivePanel('TRIAGE')} />
        )}
        {activePanel === 'INGEST' && <IngestIntel />}
        {activePanel === 'DESK' && <SyncDashboard />}
      </main>
    </div>
  );
}

export default App;
