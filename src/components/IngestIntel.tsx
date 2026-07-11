import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ChangeEvent,
  type DragEvent,
  type FC,
} from 'react';
import {
  ApiService,
  GenerateCardsResponse,
  LocalInferenceConfig,
} from '../services/api';

const MODEL_OPTIONS = [
  { label: 'llama3', value: 'llama3' },
  { label: 'mistral', value: 'mistral' },
  { label: 'phi3', value: 'phi3' },
  { label: 'llama3.2 (default)', value: 'llama3.2' },
] as const;

const TERMINAL_TICK_MS = 650;
const LOADING_LINES = [
  '> binding local inference endpoint (127.0.0.1)...',
  '> streaming source document into context window...',
  '> requesting JSON schema conformance from model...',
  '> parsing flashcard objects from NDJSON stream...',
  '> validating card_type enum constraints...',
  '> persisting cards + FSRS states to SQLite...',
  '> awaiting graph linkage expansion...',
];

type IngestPhase = 'idle' | 'running' | 'success' | 'error';

export const IngestIntel: FC = () => {
  const [sourceText, setSourceText] = useState('');
  const [model, setModel] = useState<string>('llama3.2');
  const [port, setPort] = useState<number>(11434);
  const [phase, setPhase] = useState<IngestPhase>('idle');
  const [terminalLines, setTerminalLines] = useState<string[]>([]);
  const [result, setResult] = useState<GenerateCardsResponse | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const tickRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const appendTerminalLine = useCallback((line: string) => {
    setTerminalLines((prev) => [...prev.slice(-12), line]);
  }, []);

  useEffect(() => {
    if (phase !== 'running') {
      if (tickRef.current) {
        clearInterval(tickRef.current);
        tickRef.current = null;
      }
      return;
    }

    let index = 0;
    setTerminalLines([LOADING_LINES[0]]);
    tickRef.current = setInterval(() => {
      index = (index + 1) % LOADING_LINES.length;
      setTerminalLines((prev) => [...prev.slice(-12), LOADING_LINES[index]]);
    }, TERMINAL_TICK_MS);

    return () => {
      if (tickRef.current) clearInterval(tickRef.current);
    };
  }, [phase]);

  const ingestFile = useCallback(async (file: File) => {
    const allowed = /\.(md|markdown|json|txt)$/i;
    if (!allowed.test(file.name)) {
      setErrorMessage('Unsupported file type. Use .md, .json, or .txt');
      setPhase('error');
      return;
    }
    if (file.size > 512_000) {
      setErrorMessage('File exceeds 512KB local ingestion limit.');
      setPhase('error');
      return;
    }
    const text = await file.text();
    setSourceText(text);
    appendTerminalLine(`> loaded ${file.name} (${file.size} bytes)`);
  }, [appendTerminalLine]);

  const onDrop = useCallback(
    async (event: DragEvent<HTMLDivElement>) => {
      event.preventDefault();
      setIsDragging(false);
      const file = event.dataTransfer.files[0];
      if (file) await ingestFile(file);
    },
    [ingestFile],
  );

  const onFilePick = useCallback(
    async (event: ChangeEvent<HTMLInputElement>) => {
      const file = event.target.files?.[0];
      if (file) await ingestFile(file);
      event.target.value = '';
    },
    [ingestFile],
  );

  const handleExecute = async () => {
    const trimmed = sourceText.trim();
    if (!trimmed) {
      setErrorMessage('Paste or drop a document before extraction.');
      setPhase('error');
      return;
    }

    setPhase('running');
    setResult(null);
    setErrorMessage(null);
    appendTerminalLine('> EXECUTE EXTRACTION initiated');

    const inference: LocalInferenceConfig = {
      host: '127.0.0.1',
      port,
      model,
      backend: 'ollama_chat',
    };

    try {
      const response = await ApiService.generateCardsFromText(trimmed, inference);
      appendTerminalLine(`> OK: ${response.cards.length} card(s) committed to native store`);
      setResult(response);
      setPhase('success');
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      appendTerminalLine(`> ERR: ${message}`);
      setErrorMessage(message);
      setPhase('error');
    }
  };

  const charCount = sourceText.length;

  return (
    <div className="flex h-full w-full bg-slate-950 text-slate-100 font-mono">
      {/* Input column */}
      <div className="flex w-3/5 flex-col border-r border-slate-800 p-6">
        <div className="mb-4">
          <div className="text-xs font-bold uppercase tracking-wider text-teal-400">
            // INGEST RISK INTEL
          </div>
          <p className="mt-1 text-[11px] leading-relaxed text-slate-500">
            Feed markdown incident reports, sanitized JSON log samples, or regulatory whitepapers into
            the local Ollama pipeline. Output is structured flashcards persisted to SQLite.
          </p>
        </div>

        {/* Model + port config */}
        <div className="mb-4 grid grid-cols-2 gap-3">
          <label className="flex flex-col gap-1 text-[10px] text-slate-500">
            TARGET_MODEL
            <select
              value={model}
              onChange={(e) => setModel(e.target.value)}
              disabled={phase === 'running'}
              className="rounded border border-slate-800 bg-slate-900 px-3 py-2 text-xs text-emerald-400 focus:border-teal-500 focus:outline-none disabled:opacity-50"
            >
              {MODEL_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {opt.label}
                </option>
              ))}
            </select>
          </label>
          <label className="flex flex-col gap-1 text-[10px] text-slate-500">
            OLLAMA_PORT
            <input
              type="number"
              min={1}
              max={65535}
              value={port}
              onChange={(e) => setPort(Number(e.target.value) || 11434)}
              disabled={phase === 'running'}
              className="rounded border border-slate-800 bg-slate-900 px-3 py-2 text-xs text-emerald-400 focus:border-teal-500 focus:outline-none disabled:opacity-50"
            />
          </label>
        </div>

        {/* Drop zone + textarea */}
        <div
          onDragOver={(e) => {
            e.preventDefault();
            setIsDragging(true);
          }}
          onDragLeave={() => setIsDragging(false)}
          onDrop={onDrop}
          className={`relative mb-3 flex flex-1 flex-col overflow-hidden rounded-lg border transition ${
            isDragging
              ? 'border-teal-500 bg-teal-950/20'
              : 'border-slate-800 bg-slate-900/40'
          }`}
        >
          <textarea
            value={sourceText}
            onChange={(e) => setSourceText(e.target.value)}
            disabled={phase === 'running'}
            placeholder="Paste raw intelligence here, or drop .md / .json / .txt files..."
            spellCheck={false}
            className="min-h-0 flex-1 resize-none bg-transparent p-4 text-xs leading-relaxed text-slate-300 placeholder:text-slate-600 focus:outline-none disabled:opacity-60"
          />
          <div className="flex items-center justify-between border-t border-slate-800 px-4 py-2 text-[10px] text-slate-600">
            <button
              type="button"
              onClick={() => fileInputRef.current?.click()}
              disabled={phase === 'running'}
              className="text-teal-500 hover:text-teal-400 disabled:opacity-50"
            >
              [+] IMPORT FILE
            </button>
            <span>{charCount.toLocaleString()} chars</span>
          </div>
          <input
            ref={fileInputRef}
            type="file"
            accept=".md,.markdown,.json,.txt,text/plain,application/json"
            className="hidden"
            onChange={onFilePick}
          />
        </div>

        <button
          type="button"
          onClick={handleExecute}
          disabled={phase === 'running' || !sourceText.trim()}
          className="rounded bg-teal-700 py-3 text-xs font-bold tracking-wide text-white shadow transition hover:bg-teal-600 disabled:cursor-not-allowed disabled:opacity-40"
        >
          {phase === 'running' ? 'EXTRACTING...' : 'EXECUTE EXTRACTION'}
        </button>
      </div>

      {/* Terminal + results column */}
      <div className="flex w-2/5 flex-col p-6">
        <div className="mb-2 text-[10px] font-semibold uppercase tracking-wider text-slate-500">
          // INFERENCE TERMINAL
        </div>
        <div className="mb-4 flex-1 overflow-y-auto rounded-lg border border-slate-800 bg-black/60 p-4 text-[11px] leading-relaxed shadow-inner">
          {terminalLines.length === 0 ? (
            <span className="text-slate-600">Awaiting extraction command...</span>
          ) : (
            terminalLines.map((line, i) => (
              <div
                key={`${i}-${line}`}
                className={`mb-1 ${
                  line.startsWith('> ERR')
                    ? 'text-rose-400'
                    : line.startsWith('> OK')
                      ? 'text-emerald-400'
                      : 'text-slate-400'
                } ${phase === 'running' && i === terminalLines.length - 1 ? 'animate-pulse' : ''}`}
              >
                {line}
              </div>
            ))
          )}
          {phase === 'running' && (
            <div className="mt-2 inline-block h-3 w-2 animate-pulse bg-teal-500" aria-hidden />
          )}
        </div>

        {phase === 'success' && result && (
          <div className="animate-fadeIn rounded-lg border border-emerald-900/60 bg-emerald-950/30 p-4">
            <div className="mb-2 text-xs font-bold text-emerald-400">
              EXTRACTION COMPLETE — {result.cards.length} CARD(S)
            </div>
            <ul className="max-h-40 space-y-2 overflow-y-auto text-[10px] text-slate-400">
              {result.cards.map((card) => (
                <li
                  key={card.id}
                  className="rounded border border-slate-800 bg-slate-900/50 px-2 py-1.5"
                >
                  <span className="text-teal-400">[{card.card_type}]</span>{' '}
                  {typeof card.data.question === 'string'
                    ? card.data.question.slice(0, 80)
                    : card.id.slice(0, 8)}
                </li>
              ))}
            </ul>
          </div>
        )}

        {phase === 'error' && errorMessage && (
          <div className="rounded-lg border border-rose-900/60 bg-rose-950/30 p-4 text-xs text-rose-400">
            {errorMessage}
          </div>
        )}
      </div>
    </div>
  );
};
