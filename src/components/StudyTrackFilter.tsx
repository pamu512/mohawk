import type { FC } from 'react';
import { STUDY_TRACKS, type StudyTrack } from '../lib/studyTracks';

interface StudyTrackFilterProps {
  active: StudyTrack;
  onChange: (track: StudyTrack) => void;
  queueCount: number;
}

export const StudyTrackFilter: FC<StudyTrackFilterProps> = ({
  active,
  onChange,
  queueCount,
}) => (
  <div className="border-b border-slate-800 bg-slate-950/90 px-4 py-3">
    <div className="mb-2 flex items-center justify-between">
      <span className="text-[10px] font-bold uppercase tracking-wider text-slate-500">
        // Curriculum Track Filter
      </span>
      <span className="text-[10px] text-slate-600">FILTERED_QUEUE: {queueCount}</span>
    </div>
    <div className="flex flex-wrap gap-1.5">
      {STUDY_TRACKS.map(({ id, label }) => (
        <button
          key={id}
          type="button"
          onClick={() => onChange(id)}
          className={`rounded border px-2.5 py-1.5 text-[10px] font-bold tracking-wide transition ${
            active === id
              ? 'border-amber-700 bg-amber-950/50 text-amber-400 shadow'
              : 'border-slate-800 bg-slate-900/50 text-slate-500 hover:border-slate-600 hover:text-slate-300'
          }`}
        >
          [{label}]
        </button>
      ))}
    </div>
  </div>
);
