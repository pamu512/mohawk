import type { Card, ChallengeLanguage } from '../services/api';

export type StudyTrack =
  | 'ALL'
  | 'PAYMENTS'
  | 'ACCOUNT_SECURITY'
  | 'TRUST_SAFETY'
  | 'SQL_LABS'
  | 'CODE_EXECUTIONS';

export const STUDY_TRACKS: { id: StudyTrack; label: string }[] = [
  { id: 'ALL', label: 'ALL' },
  { id: 'PAYMENTS', label: 'PAYMENTS' },
  { id: 'ACCOUNT_SECURITY', label: 'ACCOUNT SECURITY' },
  { id: 'TRUST_SAFETY', label: 'TRUST & SAFETY' },
  { id: 'SQL_LABS', label: 'SQL LABS' },
  { id: 'CODE_EXECUTIONS', label: 'CODE EXECUTIONS' },
];

const CODE_CATEGORIES = new Set(['python', 'r', 'statistical_analysis']);

export function matchesStudyTrack(card: Card, track: StudyTrack): boolean {
  switch (track) {
    case 'ALL':
      return true;
    case 'PAYMENTS':
      return card.category === 'payments';
    case 'ACCOUNT_SECURITY':
      return card.category === 'account_security';
    case 'TRUST_SAFETY':
      return card.category === 'trust_safety';
    case 'SQL_LABS':
      return card.category === 'sql' || (card.card_type === 'logic_sandbox' && card.category === 'sql');
    case 'CODE_EXECUTIONS':
      return (
        card.card_type === 'logic_sandbox' &&
        card.category != null &&
        CODE_CATEGORIES.has(card.category)
      );
    default:
      return true;
  }
}

export function categoryToLanguage(category: string | null | undefined): ChallengeLanguage {
  switch (category) {
    case 'sql':
      return 'SQL';
    case 'python':
      return 'Python';
    case 'r':
      return 'R';
    case 'statistical_analysis':
      return 'Stats';
    default:
      return 'SQL';
  }
}

export function objectiveForCard(card: Card): string {
  if (typeof card.data.objective === 'string') {
    return card.data.objective;
  }
  const byCategory: Record<string, string> = {
    payments:
      'Objective: Isolate velocity spikes while maintaining a False Positive Rate < 1.0%.',
    account_security:
      'Objective: Detect credential-stuffing ATO waves with high recall and FPR < 1.0%.',
    trust_safety:
      'Objective: Classify trust & safety abuse signals with precision-first thresholds (FPR < 1.0%).',
    sql: 'Objective: Write a windowed self-join SQL query — validate against 500 mock payloads with FPR < 1.0%.',
    python:
      'Objective: Implement outlier detection logic in Python — maximize fraud capture at FPR < 1.0%.',
    r: 'Objective: Apply robust statistical filters in R — target FPR < 1.0% on mock fraud stream.',
    statistical_analysis:
      'Objective: Implement IQR / quantile outlier filtering — FPR must remain below 1.0%.',
  };
  return (
    byCategory[card.category ?? ''] ??
    'Objective: Pass syntax validation and achieve False Positive Rate < 1.0% on the evaluation matrix.'
  );
}

export function starterCodeForCard(card: Card): string {
  const data = card.data;
  if (typeof data.starter_sql === 'string') return data.starter_sql;
  if (typeof data.starter_python === 'string') return data.starter_python;
  if (typeof data.starter_r === 'string') return data.starter_r;
  if (card.category === 'sql') {
    return '-- SELECT device_token, COUNT(DISTINCT account_id)\n-- FROM auth_events e1\n-- JOIN auth_events e2 ON ...\n';
  }
  if (card.category === 'statistical_analysis' || card.category === 'python') {
    return 'def iqr_filter(amounts, k=1.5):\n    """Return inlier transaction amounts."""\n    ...\n';
  }
  return '';
}
