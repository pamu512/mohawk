import { useState, useEffect, type FC } from 'react';
import {
  ApiService,
  Card,
  ChallengeLanguage,
  SandboxResult,
} from '../services/api';
import {
  categoryToLanguage,
  objectiveForCard,
  starterCodeForCard,
} from '../lib/studyTracks';

interface TechnicalLabConsoleProps {
  card: Card;
  onSubmitScore: (rating: number) => void;
}

type LabPhase = 'coding' | 'running' | 'results';

export const TechnicalLabConsole: FC<TechnicalLabConsoleProps> = ({
  card,
  onSubmitScore,
}) => {
  const [code, setCode] = useState(() => starterCodeForCard(card));
  const [language, setLanguage] = useState<ChallengeLanguage>(() =>
    categoryToLanguage(card.category),
  );
  const [phase, setPhase] = useState<LabPhase>('coding');
  const [result, setResult] = useState<SandboxResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const objective = objectiveForCard(card);

  useEffect(() => {
    setCode(starterCodeForCard(card));
    setLanguage(categoryToLanguage(card.category));
    setPhase('coding');
    setResult(null);
    setError(null);
  }, [card.id, card.category]);

  const handleRun = async () => {
    setPhase('running');
    setError(null);
    setResult(null);
    try {
      const telemetry = await ApiService.executeSandboxRules(code, [], language);
      setResult(telemetry);
      setPhase('results');
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setPhase('coding');
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-col gap-4">
      <div className="rounded border border-cyan-900/50 bg-cyan-950/20 px-4 py-3">
        <div className="mb-1 flex flex-wrap items-center gap-2 text-[10px]">
          <span className="rounded border border-cyan-800 bg-cyan-950 px-2 py-0.5 font-bold text-cyan-400">
            TECHNICAL LAB CONSOLE
          </span>
          {card.category && (
            <span className="text-slate-500">TRACK::{card.category.toUpperCase()}</span>
          )}
          {card.difficulty_tier && (
            <span className="text-amber-600">TIER::{card.difficulty_tier.toUpperCase()}</span>
          )}
        </div>
        <p className="text-[11px] leading-relaxed text-cyan-200/80">{objective}</p>
      </div>

      <h2 className="text-sm font-semibold leading-snug text-slate-300">
        {typeof card.data.question === 'string'
          ? card.data.question
          : 'Complete the technical challenge below.'}
      </h2>

      {card.data.payload != null && (
        <pre className="max-h-28 overflow-auto rounded border border-slate-800 bg-black/40 p-3 text-[10px] leading-relaxed text-emerald-500/90">
          {JSON.stringify(card.data.payload, null, 2)}
        </pre>
      )}

      <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-slate-700 bg-black shadow-inner">
        <div className="flex items-center justify-between border-b border-slate-800 bg-slate-900 px-3 py-2">
          <div className="flex items-center gap-2 text-[10px] text-slate-500">
            <span className="text-red-400">●</span>
            <span className="text-amber-400">●</span>
            <span className="text-emerald-400">●</span>
            <span className="ml-2 font-bold text-slate-400">mohawk-lab — {language}</span>
          </div>
          <select
            value={language}
            onChange={(e) => setLanguage(e.target.value as ChallengeLanguage)}
            disabled={phase === 'running'}
            className="rounded border border-slate-700 bg-slate-950 px-2 py-1 text-[10px] text-emerald-400 focus:border-cyan-600 focus:outline-none disabled:opacity-50"
          >
            <option value="SQL">SQL</option>
            <option value="Python">Python</option>
            <option value="R">R</option>
            <option value="Stats">Stats</option>
          </select>
        </div>
        <textarea
          value={code}
          onChange={(e) => setCode(e.target.value)}
          disabled={phase === 'running'}
          spellCheck={false}
          className="min-h-[200px] flex-1 resize-none bg-black p-4 font-mono text-xs leading-relaxed text-emerald-400 caret-cyan-400 focus:outline-none disabled:opacity-60"
          aria-label="Technical lab code editor"
        />
      </div>

      <div className="flex flex-wrap items-center gap-3">
        <button
          type="button"
          onClick={handleRun}
          disabled={phase === 'running' || !code.trim()}
          className="rounded bg-cyan-700 px-6 py-2.5 text-xs font-bold tracking-wide text-white transition hover:bg-cyan-600 disabled:cursor-not-allowed disabled:opacity-40"
        >
          {phase === 'running' ? 'VALIDATING AGAINST 500 MOCK PAYLOADS...' : '▶ RUN VALIDATION'}
        </button>
        {error && <span className="text-[11px] text-rose-400">{error}</span>}
      </div>

      {phase === 'results' && result && (
        <div className="rounded-lg border border-slate-800 bg-slate-900/60 p-4 animate-fadeIn">
          <div className="mb-3 text-[10px] font-bold uppercase tracking-wider text-slate-500">
            // Execution Telemetry
          </div>
          {!result.syntax_valid && (
            <ul className="mb-3 space-y-1 text-[10px] text-rose-400">
              {result.validation_errors.map((msg) => (
                <li key={msg}>✗ {msg}</li>
              ))}
            </ul>
          )}
          <div className="grid grid-cols-2 gap-2 text-[10px] sm:grid-cols-4">
            <div className="rounded border border-slate-800 bg-black/40 p-2">
              <div className="text-slate-600">TRUE_POS</div>
              <div className="text-lg font-bold text-emerald-400">{result.true_positives}</div>
            </div>
            <div className="rounded border border-slate-800 bg-black/40 p-2">
              <div className="text-slate-600">FALSE_POS</div>
              <div className="text-lg font-bold text-rose-400">{result.false_positives}</div>
            </div>
            <div className="rounded border border-slate-800 bg-black/40 p-2">
              <div className="text-slate-600">FRAUD_CAUGHT</div>
              <div className="text-lg font-bold text-amber-400">
                {result.fraud_caught_percentage.toFixed(1)}%
              </div>
            </div>
            <div className="rounded border border-slate-800 bg-black/40 p-2">
              <div className="text-slate-600">FPR</div>
              <div
                className={`text-lg font-bold ${
                  result.false_positive_percentage > 1.0 ? 'text-rose-400' : 'text-emerald-400'
                }`}
              >
                {result.false_positive_percentage.toFixed(2)}%
              </div>
            </div>
          </div>
          <div className="mt-2 text-[10px] text-slate-600">
            {result.total_evaluated} payloads · {result.execution_time_ms.toFixed(2)}ms ·{' '}
            {result.challenge_language ?? language}
          </div>

          {typeof card.data.answer === 'string' && (
            <div className="mt-4 border-t border-slate-800 pt-4">
              <div className="mb-1 text-[10px] font-bold text-emerald-500">REFERENCE SOLUTION</div>
              <p className="text-[11px] leading-relaxed text-slate-400">{card.data.answer}</p>
            </div>
          )}

          <div className="mt-4 flex gap-2">
            {[1, 2, 3, 4].map((rating) => (
              <button
                key={rating}
                type="button"
                onClick={() => onSubmitScore(rating)}
                className="flex-1 rounded border border-slate-700 bg-slate-950 py-2 text-[10px] font-bold text-slate-400 transition hover:border-cyan-700 hover:text-cyan-300"
              >
                [{rating}] SCORE
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
};
