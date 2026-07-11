import { useCallback, useEffect, useMemo, useRef, useState, type FC } from 'react';
import { ApiService, Card } from '../services/api';
import { matchesStudyTrack, type StudyTrack } from '../lib/studyTracks';
import { StudyTrackFilter } from './StudyTrackFilter';
import { TechnicalLabConsole } from './TechnicalLabConsole';

type CircuitState = 'LOADING' | 'IDLE' | 'TRIAGE' | 'REVEALED';

interface StudyCircuitProps {
  initialTrack?: StudyTrack;
}

export const StudyCircuit: FC<StudyCircuitProps> = ({ initialTrack = 'ALL' }) => {
  const [viewState, setViewState] = useState<CircuitState>('LOADING');
  const [allCards, setAllCards] = useState<Card[]>([]);
  const [activeTrack, setActiveTrack] = useState<StudyTrack>(initialTrack);
  const [currentIndex, setCurrentIndex] = useState(0);
  const [timeLeft, setTimeLeft] = useState(15);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const filteredQueue = useMemo(
    () => allCards.filter((c) => matchesStudyTrack(c, activeTrack)),
    [allCards, activeTrack],
  );

  const currentCard = filteredQueue[currentIndex] ?? null;
  const isLabCard = currentCard?.card_type === 'logic_sandbox';

  const loadQueue = useCallback(async () => {
    setViewState('LOADING');
    try {
      const cards = await ApiService.getNextReviewCards(50);
      setAllCards(cards);
      setCurrentIndex(0);
      setViewState(cards.length > 0 ? 'TRIAGE' : 'IDLE');
    } catch (err) {
      console.error('Failed to bootstrap triage queue:', err);
      setViewState('IDLE');
    }
  }, []);

  useEffect(() => {
    loadQueue();
  }, [loadQueue]);

  useEffect(() => {
    setCurrentIndex(0);
    setTimeLeft(15);
    if (filteredQueue.length > 0) {
      setViewState('TRIAGE');
    } else if (allCards.length > 0) {
      setViewState('IDLE');
    }
  }, [activeTrack, filteredQueue.length, allCards.length]);

  useEffect(() => {
    if (viewState !== 'TRIAGE' || isLabCard || timeLeft <= 0) {
      if (timerRef.current) clearInterval(timerRef.current);
      return;
    }

    timerRef.current = setInterval(() => {
      setTimeLeft((prev) => {
        if (prev <= 1) {
          if (timerRef.current) clearInterval(timerRef.current);
          setViewState('REVEALED');
          return 0;
        }
        return prev - 1;
      });
    }, 1000);

    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [viewState, timeLeft, isLabCard, currentCard?.id]);

  const advanceQueue = useCallback(() => {
    setAllCards((prev) => prev.filter((c) => c.id !== currentCard?.id));
    setCurrentIndex(0);
    setTimeLeft(15);
    setViewState('TRIAGE');
  }, [currentCard?.id]);

  const handleReveal = () => {
    if (timerRef.current) clearInterval(timerRef.current);
    setViewState('REVEALED');
  };

  const handleScoreSubmission = async (rating: number) => {
    if (!currentCard) return;
    try {
      await ApiService.submitReviewScore(currentCard.id, rating);
      const remaining = filteredQueue.filter((c) => c.id !== currentCard.id);
      if (remaining.length > 0) {
        advanceQueue();
      } else {
        setAllCards((prev) => prev.filter((c) => c.id !== currentCard.id));
        setViewState('IDLE');
      }
    } catch (err) {
      console.error('Error submitting evaluation performance:', err);
    }
  };

  if (viewState === 'LOADING') {
    return (
      <div className="flex h-full w-full flex-col">
        <StudyTrackFilter active={activeTrack} onChange={setActiveTrack} queueCount={0} />
        <div className="flex flex-1 items-center justify-center bg-slate-950 font-mono text-sm text-slate-400">
          Initializing native database pools and FSRS queues...
        </div>
      </div>
    );
  }

  if (viewState === 'IDLE') {
    return (
      <div className="flex h-full w-full flex-col">
        <StudyTrackFilter active={activeTrack} onChange={setActiveTrack} queueCount={0} />
        <div className="flex flex-1 flex-col items-center justify-center bg-slate-950 px-6 text-center font-mono">
          <div className="mb-2 text-xl text-emerald-400">✓ All Systems Nominal</div>
          <p className="max-w-md text-sm text-slate-500">
            {activeTrack === 'ALL'
              ? 'The fraud vector triage queue is empty.'
              : `No due cards in the [${activeTrack}] track. Try ALL or another curriculum filter.`}
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0 w-full flex-col bg-slate-950 font-mono text-slate-100 selection:bg-amber-500/30">
      <StudyTrackFilter
        active={activeTrack}
        onChange={setActiveTrack}
        queueCount={filteredQueue.length}
      />

      <div className="flex min-h-0 flex-1 flex-col p-6">
        <div className="mb-4 flex items-center justify-between border-b border-slate-800 pb-4">
          <div className="flex flex-wrap items-center gap-3">
            <span className="rounded border border-slate-700 bg-slate-900 px-2 py-1 text-xs text-slate-400">
              QUEUE_DEPTH: {filteredQueue.length}
            </span>
            <span className="text-xs text-slate-500">
              CARD_ID: {currentCard?.id.substring(0, 12)}
            </span>
            {currentCard?.category && (
              <span className="rounded border border-slate-800 px-2 py-0.5 text-[10px] text-cyan-500">
                {currentCard.category.toUpperCase()}
              </span>
            )}
            <span className="text-[10px] text-slate-600">
              MODE: {isLabCard ? 'TECHNICAL_LAB' : 'VISUAL_DRILL'}
            </span>
          </div>
          {!isLabCard && (
            <div
              className={`text-lg font-bold ${
                timeLeft <= 5 ? 'animate-pulse text-rose-500' : 'text-amber-500'
              }`}
            >
              TTL: {timeLeft}s
            </div>
          )}
        </div>

        <div className="min-h-0 flex-1 overflow-auto">
          {isLabCard && currentCard ? (
            <TechnicalLabConsole card={currentCard} onSubmitScore={handleScoreSubmission} />
          ) : (
            <div className="rounded-lg border border-slate-800 bg-slate-900/50 p-6 shadow-inner">
              <div className="mb-2 text-xs uppercase tracking-wider text-amber-400/80">
                [!] INCOMING FRAUD ALIGNMENT PAYLOAD
              </div>
              <h2 className="mb-4 text-base font-semibold text-slate-300">
                {(typeof currentCard?.data?.question === 'string'
                  ? currentCard.data.question
                  : null) || 'Analyze the following payload anomaly:'}
              </h2>

              {currentCard?.data?.payload != null && (
                <pre className="overflow-x-auto rounded border border-slate-800 bg-slate-950 p-4 text-xs leading-relaxed text-emerald-400/90">
                  {JSON.stringify(currentCard.data.payload, null, 2)}
                </pre>
              )}

              {viewState === 'REVEALED' && (
                <div className="mt-6 animate-fadeIn border-t border-slate-800 pt-6">
                  <div className="mb-2 text-xs uppercase tracking-wider text-emerald-400">
                    [✓] ROOT CAUSE ANALYSIS
                  </div>
                  <p className="rounded border border-slate-900 bg-slate-950/40 p-4 text-sm leading-relaxed text-slate-300">
                    {typeof currentCard?.data?.answer === 'string'
                      ? currentCard.data.answer
                      : ''}
                  </p>
                </div>
              )}
            </div>
          )}
        </div>

        {!isLabCard && (
          <div className="mt-4 flex h-16 items-center justify-center">
            {viewState === 'TRIAGE' ? (
              <button
                type="button"
                onClick={handleReveal}
                className="w-full max-w-md rounded border border-slate-700 bg-slate-900 px-6 py-3 text-sm font-medium tracking-wide text-slate-300 shadow transition hover:border-amber-500/50 hover:bg-slate-800"
              >
                EXECUTE TRACE (REVEAL ROOT CAUSE)
              </button>
            ) : (
              <div className="flex w-full max-w-xl animate-slideUp gap-3">
                {([1, 2, 3, 4] as const).map((rating) => (
                  <button
                    key={rating}
                    type="button"
                    onClick={() => handleScoreSubmission(rating)}
                    className={`flex-1 rounded border py-3 text-xs font-bold transition ${
                      rating === 1
                        ? 'border-rose-800 bg-rose-950/40 text-rose-400 hover:bg-rose-900/60'
                        : rating === 2
                          ? 'border-orange-800 bg-orange-950/40 text-orange-400 hover:bg-orange-900/60'
                          : rating === 3
                            ? 'border-indigo-800 bg-indigo-950/40 text-indigo-400 hover:bg-indigo-900/60'
                            : 'border-emerald-800 bg-emerald-950/40 text-emerald-400 hover:bg-emerald-900/60'
                    }`}
                  >
                    [{rating}]{' '}
                    {rating === 1
                      ? 'BREACHED'
                      : rating === 2
                        ? 'EVADED'
                        : rating === 3
                          ? 'MITIGATED'
                          : 'CONTAINED'}
                  </button>
                ))}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
};
