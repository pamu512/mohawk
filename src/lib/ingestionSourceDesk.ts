type CatalogSourceMark = {
  label: string;
  known_not_serving_rss: boolean;
};

export type IngestionSourceDeskPresentation = {
  dotClass: string;
  note: string | null;
};

const NOT_SERVING_DOT = 'bg-amber-600';
const LISTED_DOT = 'bg-slate-500';
const SYNCING_DOT = 'animate-pulse bg-cyan-400';

export function ingestionSourceDeskPresentation(
  source: CatalogSourceMark,
  syncInProgress: boolean,
): IngestionSourceDeskPresentation {
  if (source.known_not_serving_rss) {
    return {
      dotClass: NOT_SERVING_DOT,
      note: `${source.label} is not serving RSS (known not serving, not a live health check).`,
    };
  }
  if (syncInProgress) {
    return { dotClass: SYNCING_DOT, note: null };
  }
  return { dotClass: LISTED_DOT, note: null };
}
