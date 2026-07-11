import React, { useState } from 'react';
import { ApiService, SandboxResult } from '../services/api';

export const RuleSandbox: React.FC = () => {
  const [ruleInput, setRuleInput] = useState<string>(
    `// Write conditional evaluation logic\nIF transaction.velocity_1h > 5\nAND transaction.device_velocity_30m > 2\nTHEN action = BLOCK;`
  );
  
  const [telemetry, setTelemetry] = useState<SandboxResult | null>(null);
  const [isEvaluating, setIsEvaluating] = useState<boolean>(false);

  // Anonymized mock traffic logs to simulate an active attack wave
  const [mockPayloads] = useState<string[]>([
    JSON.stringify({ id: "tx_101", velocity_1h: 6, device_velocity_30m: 3, amount: 450 }),
    JSON.stringify({ id: "tx_102", velocity_1h: 1, device_velocity_30m: 0, amount: 20 }),
    JSON.stringify({ id: "tx_103", velocity_1h: 12, device_velocity_30m: 5, amount: 890 }),
    JSON.stringify({ id: "tx_104", velocity_1h: 2, device_velocity_30m: 1, amount: 110 }),
  ]);

  const handleCompileAndRun = async () => {
    setIsEvaluating(true);
    try {
      const result = await ApiService.executeSandboxRules(ruleInput, mockPayloads);
      setTelemetry(result);
    } catch (err) {
      console.error("Rule validation compilation aborted:", err);
    } finally {
      setIsEvaluating(false);
    }
  };

  return (
    <div className="flex h-full w-full bg-slate-950 font-mono text-slate-200">
      {/* Left Input Pane: Rule Logic Compiler */}
      <div className="w-1/2 flex flex-col border-r border-slate-800 p-4">
        <div className="text-xs font-semibold text-indigo-400 tracking-wider uppercase mb-2">
          // HEURISTIC RULE INTERPRETER
        </div>
        <textarea
          value={ruleInput}
          onChange={(e) => setRuleInput(e.target.value)}
          className="flex-1 bg-slate-900 border border-slate-800 rounded p-4 text-xs font-mono text-emerald-400 focus:outline-none focus:border-indigo-500 resize-none leading-relaxed"
          spellCheck={false}
        />
        <button
          onClick={handleCompileAndRun}
          disabled={isEvaluating}
          className="mt-4 bg-indigo-600 hover:bg-indigo-500 text-white font-medium text-xs py-3 rounded transition shadow disabled:opacity-50"
        >
          {isEvaluating ? "COMPILING RULE OBJECTS..." : "COMPILE & INJECT INTO TRANSACTION VALVE"}
        </button>
      </div>

      {/* Right Output Pane: Telemetry Metrics */}
      <div className="w-1/2 flex flex-col p-4 bg-slate-950">
        <div className="text-xs font-semibold text-slate-400 tracking-wider uppercase mb-2">
          // LIVE METRIC STREAM DETECTOR
        </div>

        {telemetry ? (
          <div className="flex-1 flex flex-col gap-4 animate-fadeIn">
            {/* KPI Performance Metrics Layout Grid */}
            <div className="grid grid-cols-2 gap-3">
              <div className="bg-slate-900 p-4 rounded border border-slate-800">
                <div className="text-[10px] text-slate-500">TOTAL_EVALUATED</div>
                <div className="text-xl font-bold text-slate-100">{telemetry.total_evaluated}</div>
              </div>
              <div className="bg-slate-900 p-4 rounded border border-slate-800">
                <div className="text-[10px] text-slate-500">RULES_TRIGGERED</div>
                <div className="text-xl font-bold text-amber-400">{telemetry.rules_triggered}</div>
              </div>
              <div className="bg-slate-900 p-4 rounded border border-slate-800">
                <div className="text-[10px] text-slate-500">FRAUD_CAUGHT</div>
                <div className="text-xl font-bold text-emerald-400">{telemetry.fraud_caught_percentage}%</div>
              </div>
              <div className="bg-slate-900 p-4 rounded border border-slate-800">
                <div className="text-[10px] text-slate-500">FALSE_POSITIVE_RATE</div>
                <div className={`text-xl font-bold ${telemetry.false_positive_percentage > 1.0 ? 'text-rose-500' : 'text-emerald-400'}`}>
                  {telemetry.false_positive_percentage}%
                </div>
              </div>
            </div>

            {/* Optimization Status Report Block */}
            <div className="flex-1 bg-slate-900/40 border border-slate-800 rounded p-4 text-xs overflow-y-auto">
              <div className="text-slate-400 uppercase font-semibold mb-2">[!] COMPILER PERFORMANCE REPORT:</div>
              <div className="text-slate-500 mb-1">Execution Speed: {telemetry.execution_time_ms}ms</div>
              <div className="mt-3 text-slate-300 leading-relaxed">
                {telemetry.false_positive_percentage > 1.0 ? (
                  <span className="text-rose-400">
                    CRITICAL WARNING: False positive rate exceeds 1.0%. This logic will cause severe payment conversion friction in production pipelines. Refine transaction threshold parameters.
                  </span>
                ) : (
                  <span className="text-emerald-400">
                    SUCCESS: Heuristic profile meets performance target specs. High fraud capture efficiency coupled with sub-1% legitimate customer friction rates.
                  </span>
                )}
              </div>
            </div>
          </div>
        ) : (
          <div className="flex-1 flex items-center justify-center border border-dashed border-slate-800 rounded text-slate-600 text-xs text-center px-6">
            Awaiting rule matrix injection stream to compile performance parameters.
          </div>
        )}
      </div>
    </div>
  );
};
