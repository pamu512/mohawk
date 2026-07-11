import { useEffect, useState, type FC, type FormEvent } from 'react';
import {
  ApiService,
  GraphEdge,
  GraphNode,
  LinkedCardSummary,
} from '../services/api';

interface NodeIntelDrawerProps {
  node: GraphNode;
  edges: GraphEdge[];
  allNodes: GraphNode[];
  onClose: () => void;
  onNavigateToTriage?: () => void;
}

const ENTITY_ACCENT: Record<GraphNode['entity_type'], string> = {
  vector: 'border-rose-800 text-rose-400',
  indicator: 'border-amber-800 text-amber-400',
  legal: 'border-indigo-800 text-indigo-400',
  pattern: 'border-slate-700 text-slate-400',
};

function slugField(title: string): string {
  return title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '_')
    .replace(/^_|_$/g, '')
    .slice(0, 40);
}

function indicatorRule(node: GraphNode): string {
  const field = slugField(node.title);
  return `IF transaction.${field} == true\nAND session.risk_score > 70\nTHEN action = REVIEW`;
}

function indicatorTelemetry(node: GraphNode): string[] {
  const base = slugField(node.title);
  return [
    `transaction.${base}`,
    'session.device_id',
    'session.ip_asn',
    'event.timestamp',
    'user.account_age_days',
  ];
}

export const NodeIntelDrawer: FC<NodeIntelDrawerProps> = ({
  node,
  edges,
  allNodes,
  onClose,
  onNavigateToTriage,
}) => {
  const [linkedCards, setLinkedCards] = useState<LinkedCardSummary[]>([]);
  const [loadingCards, setLoadingCards] = useState(true);
  const [showManualForm, setShowManualForm] = useState(false);
  const [question, setQuestion] = useState('');
  const [answer, setAnswer] = useState('');
  const [saving, setSaving] = useState(false);
  const [saveMessage, setSaveMessage] = useState<string | null>(null);

  const nodeById = new Map(allNodes.map((n) => [n.id, n]));

  const inbound = edges.filter((e) => e.target_node_id === node.id);
  const outbound = edges.filter((e) => e.source_node_id === node.id);

  const triggerIndicators = inbound
    .filter((e) => e.relationship_type === 'TRIGGERS' || e.relationship_type === 'EXPLOITS')
    .map((e) => nodeById.get(e.source_node_id))
    .filter(Boolean) as GraphNode[];

  useEffect(() => {
    let cancelled = false;
    setLoadingCards(true);
    ApiService.getNodeLinkedCards(node.id)
      .then((cards) => {
        if (!cancelled) setLinkedCards(cards);
      })
      .catch(() => {
        if (!cancelled) setLinkedCards([]);
      })
      .finally(() => {
        if (!cancelled) setLoadingCards(false);
      });
    return () => {
      cancelled = true;
    };
  }, [node.id]);

  const handleCreateManual = async (event: FormEvent) => {
    event.preventDefault();
    setSaving(true);
    setSaveMessage(null);
    try {
      const saved = await ApiService.createManualCardForNode(node.id, question, answer);
      setLinkedCards((prev) => [
        {
          id: saved.id,
          card_type: saved.card_type,
          question: typeof saved.data.question === 'string' ? saved.data.question : question,
        },
        ...prev,
      ]);
      setQuestion('');
      setAnswer('');
      setShowManualForm(false);
      setSaveMessage('Manual flashcard linked to graph node.');
    } catch (err) {
      setSaveMessage(err instanceof Error ? err.message : 'Failed to save card.');
    } finally {
      setSaving(false);
    }
  };

  const accent = ENTITY_ACCENT[node.entity_type];

  return (
    <aside
      className={`absolute top-0 right-0 z-20 flex h-full w-full max-w-md flex-col border-l border-slate-800 bg-slate-950/95 font-mono shadow-2xl backdrop-blur-md transition-transform duration-300 ease-out`}
      role="dialog"
      aria-label={`Node intelligence: ${node.title}`}
    >
      {/* Header */}
      <div className={`border-b px-5 py-4 ${accent.split(' ')[0]} border-opacity-50`}>
        <div className="flex items-start justify-between gap-3">
          <div>
            <div className={`text-[10px] font-bold uppercase tracking-wider ${accent.split(' ')[1]}`}>
              [{node.entity_type}]
            </div>
            <h2 className="mt-1 text-sm font-bold leading-snug text-slate-100">{node.title}</h2>
            <p className="mt-2 text-[11px] leading-relaxed text-slate-500">{node.description}</p>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="shrink-0 rounded border border-slate-700 px-2 py-1 text-[10px] text-slate-400 hover:bg-slate-900 hover:text-slate-200"
          >
            ESC ✕
          </button>
        </div>
      </div>

      {/* Body */}
      <div className="flex-1 overflow-y-auto px-5 py-4 space-y-5">
        {node.entity_type === 'vector' && (
          <>
            <section>
              <div className="mb-2 text-[10px] font-bold uppercase tracking-wider text-rose-400">
                // Root Cause Analysis
              </div>
              <p className="rounded border border-slate-800 bg-slate-900/60 p-3 text-[11px] leading-relaxed text-slate-400">
                {node.description}
              </p>
            </section>
            <section>
              <div className="mb-2 text-[10px] font-bold uppercase tracking-wider text-rose-400">
                // Mitigation Vectors
              </div>
              <ul className="space-y-2 text-[11px] text-slate-400">
                {triggerIndicators.length > 0 ? (
                  triggerIndicators.map((ind) => (
                    <li
                      key={ind.id}
                      className="rounded border border-slate-800 bg-slate-900/40 px-3 py-2"
                    >
                      <span className="text-amber-400">SIGNAL:</span> {ind.title}
                      <div className="mt-1 text-[10px] text-slate-600">
                        Est. loss reduction: monitor + step-up auth on trigger
                      </div>
                    </li>
                  ))
                ) : (
                  <li className="text-slate-600">No inbound indicator edges mapped.</li>
                )}
              </ul>
            </section>
            <section>
              <div className="mb-2 text-[10px] font-bold uppercase tracking-wider text-amber-400">
                // Graph Connectivity
              </div>
              <div className="text-[10px] text-slate-500">
                IN: {inbound.length} edge(s) · OUT: {outbound.length} edge(s)
              </div>
            </section>
          </>
        )}

        {node.entity_type === 'indicator' && (
          <>
            <section>
              <div className="mb-2 text-[10px] font-bold uppercase tracking-wider text-amber-400">
                // Rule Logic Formula
              </div>
              <pre className="overflow-x-auto rounded border border-slate-800 bg-black/50 p-3 text-[10px] leading-relaxed text-emerald-400/90">
                {indicatorRule(node)}
              </pre>
            </section>
            <section>
              <div className="mb-2 text-[10px] font-bold uppercase tracking-wider text-amber-400">
                // Expected Telemetry Fields
              </div>
              <ul className="space-y-1">
                {indicatorTelemetry(node).map((field) => (
                  <li
                    key={field}
                    className="rounded border border-slate-800 bg-slate-900/40 px-2 py-1 text-[10px] text-slate-400"
                  >
                    {field}
                  </li>
                ))}
              </ul>
            </section>
            {outbound.length > 0 && (
              <section>
                <div className="mb-2 text-[10px] font-bold uppercase tracking-wider text-slate-500">
                  // Downstream Targets
                </div>
                <ul className="space-y-1 text-[11px] text-slate-400">
                  {outbound.map((e) => (
                    <li key={`${e.source_node_id}-${e.relationship_type}`}>
                      {e.relationship_type} →{' '}
                      {nodeById.get(e.target_node_id)?.title ?? e.target_node_id}
                    </li>
                  ))}
                </ul>
              </section>
            )}
          </>
        )}

        {node.entity_type === 'legal' && (
          <>
            <section>
              <div className="mb-2 text-[10px] font-bold uppercase tracking-wider text-indigo-400">
                // Compliance Framework
              </div>
              <p className="rounded border border-slate-800 bg-slate-900/60 p-3 text-[11px] leading-relaxed text-slate-400">
                {node.description}
              </p>
            </section>
            <section>
              <div className="mb-2 text-[10px] font-bold uppercase tracking-wider text-indigo-400">
                // Enforcement Requirements
              </div>
              <ul className="space-y-2 text-[11px] text-slate-400">
                <li className="rounded border border-indigo-900/40 bg-indigo-950/20 px-3 py-2">
                  <span className="text-indigo-400">DEADLINE:</span> Continuous monitoring —
                  violations require documented remediation within 30 business days.
                </li>
                <li className="rounded border border-indigo-900/40 bg-indigo-950/20 px-3 py-2">
                  <span className="text-indigo-400">REQUIREMENT:</span> Maintain auditable control
                  evidence mapped to linked attack vectors in this topology.
                </li>
              </ul>
            </section>
            {outbound.length > 0 && (
              <section>
                <div className="mb-2 text-[10px] font-bold uppercase tracking-wider text-slate-500">
                  // Constrained Vectors
                </div>
                <ul className="space-y-1 text-[11px] text-slate-400">
                  {outbound.map((e) => (
                    <li key={`${e.target_node_id}-${e.relationship_type}`}>
                      {e.relationship_type} →{' '}
                      {nodeById.get(e.target_node_id)?.title ?? e.target_node_id}
                    </li>
                  ))}
                </ul>
              </section>
            )}
          </>
        )}

        {(node.entity_type === 'pattern' ||
          !['vector', 'indicator', 'legal'].includes(node.entity_type)) && (
          <section>
            <div className="mb-2 text-[10px] font-bold uppercase tracking-wider text-slate-500">
              // Node Summary
            </div>
            <p className="text-[11px] text-slate-400">{node.description}</p>
          </section>
        )}

        {/* Linked flashcards */}
        <section>
          <div className="mb-2 text-[10px] font-bold uppercase tracking-wider text-teal-400">
            // Linked Triage Flashcards ({linkedCards.length})
          </div>
          {loadingCards ? (
            <p className="text-[10px] text-slate-600">Loading card index...</p>
          ) : linkedCards.length === 0 ? (
            <p className="text-[10px] text-slate-600">No flashcards linked to this node yet.</p>
          ) : (
            <ul className="max-h-32 space-y-1 overflow-y-auto">
              {linkedCards.map((card) => (
                <li
                  key={card.id}
                  className="truncate rounded border border-slate-800 bg-slate-900/50 px-2 py-1.5 text-[10px] text-slate-400"
                >
                  <span className="text-teal-500">[{card.card_type}]</span>{' '}
                  {card.question ?? card.id.slice(0, 8)}
                </li>
              ))}
            </ul>
          )}
          {node.entity_type === 'vector' && linkedCards.length > 0 && onNavigateToTriage && (
            <button
              type="button"
              onClick={onNavigateToTriage}
              className="mt-3 w-full rounded border border-amber-800 bg-amber-950/40 py-2 text-[10px] font-bold text-amber-400 transition hover:bg-amber-900/50"
            >
              VIEW IN TRIAGE CIRCUIT →
            </button>
          )}
        </section>

        {/* Manual card form */}
        {showManualForm ? (
          <form onSubmit={handleCreateManual} className="space-y-3 rounded border border-slate-800 bg-slate-900/40 p-3">
            <div className="text-[10px] font-bold uppercase tracking-wider text-teal-400">
              // Manual Flashcard
            </div>
            <input
              value={question}
              onChange={(e) => setQuestion(e.target.value)}
              placeholder="Question / prompt"
              required
              className="w-full rounded border border-slate-800 bg-slate-950 px-2 py-2 text-[11px] text-slate-300 focus:border-teal-600 focus:outline-none"
            />
            <textarea
              value={answer}
              onChange={(e) => setAnswer(e.target.value)}
              placeholder="Answer / root cause"
              required
              rows={3}
              className="w-full resize-none rounded border border-slate-800 bg-slate-950 px-2 py-2 text-[11px] text-slate-300 focus:border-teal-600 focus:outline-none"
            />
            <div className="flex gap-2">
              <button
                type="submit"
                disabled={saving}
                className="flex-1 rounded bg-teal-800 py-2 text-[10px] font-bold text-white hover:bg-teal-700 disabled:opacity-50"
              >
                {saving ? 'LINKING...' : 'SAVE & LINK'}
              </button>
              <button
                type="button"
                onClick={() => setShowManualForm(false)}
                className="rounded border border-slate-700 px-3 text-[10px] text-slate-500 hover:text-slate-300"
              >
                CANCEL
              </button>
            </div>
          </form>
        ) : (
          <button
            type="button"
            onClick={() => setShowManualForm(true)}
            className="w-full rounded border border-teal-800 bg-teal-950/30 py-2.5 text-[10px] font-bold text-teal-400 transition hover:bg-teal-900/40"
          >
            + ADD MANUAL FLASHCARD TO NODE
          </button>
        )}

        {saveMessage && (
          <p className="text-[10px] text-emerald-400">{saveMessage}</p>
        )}
      </div>
    </aside>
  );
};
