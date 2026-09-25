//! Free Spaced Repetition Scheduler (FSRS) core calculations.
//!
//! Implements the FSRS-6 memory model (21 trainable weights) for stability (`S`) and
//! difficulty (`D`) updates. Intervals target a configurable desired retention (default 90%).
//!
//! Reference: <https://github.com/open-spaced-repetition/fsrs-rs>

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

const S_MIN: f64 = 0.001;
const S_MAX: f64 = 36_500.0;
const D_MIN: f64 = 1.0;
const D_MAX: f64 = 10.0;

/// FSRS-6 default weights (same as Anki FSRS 6.1.1 global preset).
pub const DEFAULT_WEIGHTS: [f64; 21] = [
    0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.001, 1.8722, 0.1666, 0.796, 1.4835,
    0.0614, 0.2629, 1.6483, 0.6014, 1.8729, 0.5425, 0.0912, 0.0658, 0.1542,
];

/// User performance rating on a review (1 = Again … 4 = Easy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Rating {
    Again = 1,
    Hard = 2,
    Good = 3,
    Easy = 4,
}

impl Rating {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Again),
            2 => Some(Self::Hard),
            3 => Some(Self::Good),
            4 => Some(Self::Easy),
            _ => None,
        }
    }

    fn as_f64(self) -> f64 {
        self as u8 as f64
    }
}

/// Card scheduling phase; maps to `fsrs_states.state` (0–3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum FsrsPhase {
    New = 0,
    Learning = 1,
    Review = 2,
    Relearning = 3,
}

impl FsrsPhase {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::New),
            1 => Some(Self::Learning),
            2 => Some(Self::Review),
            3 => Some(Self::Relearning),
            _ => None,
        }
    }
}

/// Full schedulable memory state for a card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardState {
    pub stability: f64,
    pub difficulty: f64,
    pub lapses: u32,
    pub reviews: u32,
    pub phase: FsrsPhase,
    pub last_review: Option<DateTime<Utc>>,
    pub next_review: Option<DateTime<Utc>>,
}

impl Default for CardState {
    fn default() -> Self {
        Self {
            stability: 0.0,
            difficulty: 0.0,
            lapses: 0,
            reviews: 0,
            phase: FsrsPhase::New,
            last_review: None,
            next_review: None,
        }
    }
}

impl CardState {
    pub fn is_new(&self) -> bool {
        self.phase == FsrsPhase::New && self.stability == 0.0
    }

    /// Memory state pair used by FSRS formulas.
    pub fn memory(&self) -> MemoryState {
        MemoryState {
            stability: self.stability,
            difficulty: self.difficulty,
        }
    }
}

/// FSRS memory vector: stability (days to 90% recall) and difficulty (1–10).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MemoryState {
    pub stability: f64,
    pub difficulty: f64,
}

/// A single review event in chronological order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewLog {
    pub rating: Rating,
    pub reviewed_at: DateTime<Utc>,
    /// Days since the previous review. Use `0.0` for same-day (intra-day) reviews.
    pub elapsed_days: f64,
}

/// Projected outcome for one rating button.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntervalOutcome {
    pub stability: f64,
    pub difficulty: f64,
    pub interval_days: f64,
    pub next_review: DateTime<Utc>,
}

/// All four scheduling branches from the current card state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchedulingIntervals {
    pub again: IntervalOutcome,
    pub hard: IntervalOutcome,
    pub good: IntervalOutcome,
    pub easy: IntervalOutcome,
}

/// FSRS parameters controlling interval length and formula weights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FsrsParams {
    pub weights: [f64; 21],
    /// Target recall probability at the next review (typically `0.9`).
    pub desired_retention: f64,
    /// Upper bound on scheduled interval in whole days.
    pub maximum_interval: u32,
}

impl Default for FsrsParams {
    fn default() -> Self {
        Self {
            weights: DEFAULT_WEIGHTS,
            desired_retention: 0.9,
            maximum_interval: 36_500,
        }
    }
}

// --- Forgetting curve ---

/// Decay exponent from weight `w[20]`.
fn decay(weights: &[f64; 21]) -> f64 {
    -weights[20]
}

/// Factor used in the power-law forgetting curve.
fn factor(weights: &[f64; 21]) -> f64 {
    (0.9_f64.ln() / decay(weights)).exp() - 1.0
}

/// Retrievability `R` at `elapsed_days` given stability `S`.
///
/// `R(t, S) = (1 + FACTOR * t / S)^DECAY`
pub fn retrievability(stability: f64, elapsed_days: f64, params: &FsrsParams) -> f64 {
    let s = stability.clamp(S_MIN, S_MAX);
    let t = elapsed_days.max(0.0);
    (t / s * factor(&params.weights) + 1.0).powf(decay(&params.weights))
}

// --- Initial values (first rating on a new card) ---

fn init_stability(rating: Rating, weights: &[f64; 21]) -> f64 {
    weights[(rating as usize) - 1].max(S_MIN)
}

/// `D0(G) = w4 - exp(w5 * (G - 1)) + 1`, clamped to [1, 10].
fn init_difficulty(rating: Rating, weights: &[f64; 21]) -> f64 {
    constrain_difficulty(
        weights[4] - (weights[5] * (rating.as_f64() - 1.0)).exp() + 1.0,
    )
}

fn constrain_difficulty(d: f64) -> f64 {
    d.clamp(D_MIN, D_MAX)
}

// --- Difficulty update ---

fn linear_damping(delta_d: f64, old_d: f64) -> f64 {
    (10.0 - old_d) * delta_d / 9.0
}

fn next_difficulty(difficulty: f64, rating: Rating, weights: &[f64; 21]) -> f64 {
    let delta_d = -weights[6] * (rating.as_f64() - 3.0);
    difficulty + linear_damping(delta_d, difficulty)
}

/// Mean reversion toward the initial difficulty of an Easy first rating.
///
/// py-fsrs parity: the reversion anchor is the *unclamped* initial-Easy
/// difficulty (`_initial_difficulty(..., clamp=False)`); clamping it to
/// D_MIN first (as `init_difficulty` does) skews every subsequent D update.
fn mean_reversion(new_d: f64, weights: &[f64; 21]) -> f64 {
    let unclamped_d0_easy = weights[4] - (weights[5] * (Rating::Easy.as_f64() - 1.0)).exp() + 1.0;
    weights[7] * (unclamped_d0_easy - new_d) + new_d
}

// --- Stability updates ---

/// Stability after successful recall (Hard / Good / Easy).
fn stability_after_recall(
    last_s: f64,
    last_d: f64,
    retrievability: f64,
    rating: Rating,
    weights: &[f64; 21],
) -> f64 {
    let hard_penalty = if rating == Rating::Hard { weights[15] } else { 1.0 };
    let easy_bonus = if rating == Rating::Easy { weights[16] } else { 1.0 };
    last_s
        * (weights[8].exp()
            * (11.0 - last_d)
            * last_s.powf(-weights[9])
            * (((1.0 - retrievability) * weights[10]).exp() - 1.0)
            * hard_penalty
            * easy_bonus
            + 1.0)
}

/// Stability after a failed recall (Again).
fn stability_after_forget(last_s: f64, last_d: f64, retrievability: f64, weights: &[f64; 21]) -> f64 {
    let new_s = weights[11]
        * last_d.powf(-weights[12])
        * ((last_s + 1.0).powf(weights[13]) - 1.0)
        * ((1.0 - retrievability) * weights[14]).exp();
    let floor = last_s / (weights[17] * weights[18]).exp();
    new_s.min(floor)
}

/// Same-day (intra-day) stability update.
fn stability_short_term(last_s: f64, rating: Rating, weights: &[f64; 21]) -> f64 {
    let mut sinc = (weights[17] * (rating.as_f64() - 3.0 + weights[18])).exp() * last_s.powf(-weights[19]);
    if rating as u8 >= Rating::Hard as u8 {
        sinc = sinc.max(1.0);
    }
    last_s * sinc
}

/// Single FSRS `step`: apply one rating after `elapsed_days` since the last review.
pub fn step(
    state: MemoryState,
    elapsed_days: f64,
    rating: Rating,
    is_first_review: bool,
    params: &FsrsParams,
) -> MemoryState {
    let weights = &params.weights;
    let last_s = state.stability.clamp(S_MIN, S_MAX);
    let last_d = state.difficulty.clamp(D_MIN, D_MAX);
    let r = retrievability(last_s, elapsed_days, params);

    let mut new_s = if rating == Rating::Again {
        stability_after_forget(last_s, last_d, r, weights)
    } else {
        stability_after_recall(last_s, last_d, r, rating, weights)
    };

    // py-fsrs parity: same-day reviews (elapsed < 1 day, truncated) use the
    // short-term stability update, not the long-term recall/forget path.
    if elapsed_days < 1.0 {
        new_s = stability_short_term(last_s, rating, weights);
    }

    let mut new_d = mean_reversion(next_difficulty(last_d, rating, weights), weights);
    new_d = constrain_difficulty(new_d);

    if is_first_review && state.stability == 0.0 {
        new_s = init_stability(rating, weights);
        new_d = init_difficulty(rating, weights);
    }

    MemoryState {
        stability: new_s.clamp(S_MIN, S_MAX),
        difficulty: new_d,
    }
}

/// Replay a review history to derive the current memory state.
pub fn memory_state_from_reviews(reviews: &[ReviewLog], params: &FsrsParams) -> MemoryState {
    let mut state = MemoryState {
        stability: 0.0,
        difficulty: 0.0,
    };

    for (nth, review) in reviews.iter().enumerate() {
        state = step(
            state,
            review.elapsed_days,
            review.rating,
            nth == 0,
            params,
        );
    }

    state
}

/// Raw interval in days from stability targeting `desired_retention`.
///
/// `I(S) = S / FACTOR * (R^(1/DECAY) - 1)`
pub fn interval_days(stability: f64, params: &FsrsParams) -> f64 {
    let s = stability.clamp(S_MIN, S_MAX);
    let w = &params.weights;
    s / factor(w) * (params.desired_retention.powf(1.0 / decay(w)) - 1.0)
}

/// Clamp and round an interval to whole days for scheduling.
pub fn clamp_interval_days(raw_days: f64, params: &FsrsParams) -> f64 {
    raw_days
        .round()
        .clamp(1.0, params.maximum_interval as f64)
}

/// Convert an interval in days to an absolute UTC due time from `now`.
pub fn next_review_at(now: DateTime<Utc>, interval_days: f64) -> DateTime<Utc> {
    let seconds = (interval_days * 86_400.0).round() as i64;
    now + Duration::seconds(seconds)
}

fn outcome_from_memory(memory: MemoryState, now: DateTime<Utc>, params: &FsrsParams) -> IntervalOutcome {
    let raw = interval_days(memory.stability, params);
    let days = clamp_interval_days(raw, params);
    IntervalOutcome {
        stability: memory.stability,
        difficulty: memory.difficulty,
        interval_days: days,
        next_review: next_review_at(now, days),
    }
}

/// Preview all four rating outcomes without mutating card state.
pub fn preview_intervals(
    state: &CardState,
    now: DateTime<Utc>,
    elapsed_days: f64,
    params: &FsrsParams,
) -> SchedulingIntervals {
    let memory = state.memory();
    let is_first = state.is_new();

    let branch = |rating: Rating| {
        let next = step(memory, elapsed_days, rating, is_first, params);
        outcome_from_memory(next, now, params)
    };

    SchedulingIntervals {
        again: branch(Rating::Again),
        hard: branch(Rating::Hard),
        good: branch(Rating::Good),
        easy: branch(Rating::Easy),
    }
}

fn advance_phase(phase: FsrsPhase, rating: Rating) -> (FsrsPhase, u32) {
    let mut lapses = 0_u32;
    let next = match (phase, rating) {
        (FsrsPhase::New, Rating::Again) => FsrsPhase::Learning,
        (FsrsPhase::New, _) => FsrsPhase::Review,
        (FsrsPhase::Review, Rating::Again) => {
            lapses = 1;
            FsrsPhase::Relearning
        }
        (FsrsPhase::Review, _) => FsrsPhase::Review,
        (FsrsPhase::Learning | FsrsPhase::Relearning, Rating::Again | Rating::Hard) => phase,
        (FsrsPhase::Learning | FsrsPhase::Relearning, Rating::Good | Rating::Easy) => {
            FsrsPhase::Review
        }
    };
    (next, lapses)
}

/// Apply one review and return the updated card state with a concrete `next_review`.
pub fn apply_review(state: &CardState, log: ReviewLog, params: &FsrsParams) -> CardState {
    let elapsed = if log.elapsed_days.is_sign_positive() || state.last_review.is_none() {
        log.elapsed_days
    } else if let Some(last) = state.last_review {
        elapsed_days_between(last, log.reviewed_at)
    } else {
        log.elapsed_days
    };

    let memory = step(state.memory(), elapsed, log.rating, state.is_new(), params);
    let outcome = outcome_from_memory(memory, log.reviewed_at, params);
    let (phase, lapse_inc) = advance_phase(state.phase, log.rating);

    CardState {
        stability: memory.stability,
        difficulty: memory.difficulty,
        lapses: state.lapses + lapse_inc,
        reviews: state.reviews + 1,
        phase,
        last_review: Some(log.reviewed_at),
        next_review: Some(outcome.next_review),
    }
}

/// Fractional days between two UTC timestamps.
pub fn elapsed_days_between(from: DateTime<Utc>, to: DateTime<Utc>) -> f64 {
    to.signed_duration_since(from).num_milliseconds() as f64 / 86_400_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn init_memory_on_first_good_review() {
        let params = FsrsParams::default();
        let memory = step(
            MemoryState {
                stability: 0.0,
                difficulty: 0.0,
            },
            0.0,
            Rating::Good,
            true,
            &params,
        );

        assert!(approx(memory.stability, DEFAULT_WEIGHTS[2]));
        assert!(approx(
            memory.difficulty,
            DEFAULT_WEIGHTS[4] - (DEFAULT_WEIGHTS[5] * 2.0).exp() + 1.0
        ));
    }

    #[test]
    fn retrievability_at_stability_equals_interval() {
        let params = FsrsParams::default();
        let s = 10.0;
        let raw = interval_days(s, &params);
        let r = retrievability(s, raw, &params);
        assert!(approx(r, params.desired_retention));
    }

    #[test]
    fn preview_intervals_branch_ordering() {
        let params = FsrsParams::default();
        let state = CardState {
            stability: 10.0,
            difficulty: 5.0,
            phase: FsrsPhase::Review,
            ..Default::default()
        };
        let now = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
        let preview = preview_intervals(&state, now, 5.0, &params);

        assert!(preview.again.stability < preview.hard.stability);
        assert!(preview.hard.stability < preview.good.stability);
        assert!(preview.good.stability < preview.easy.stability);
        assert!(preview.good.next_review > now);
    }

    #[test]
    fn apply_review_sets_next_review_datetime() {
        let params = FsrsParams::default();
        let now = Utc.with_ymd_and_hms(2025, 6, 1, 12, 0, 0).unwrap();
        let state = CardState::default();
        let updated = apply_review(
            &state,
            ReviewLog {
                rating: Rating::Good,
                reviewed_at: now,
                elapsed_days: 0.0,
            },
            &params,
        );

        assert!(updated.next_review.is_some());
        assert!(updated.last_review == Some(now));
        assert!(updated.stability > 0.0);
        assert!(updated.difficulty >= D_MIN);
    }

    #[test]
    fn rating_from_u8_rejects_invalid() {
        assert_eq!(Rating::from_u8(3), Some(Rating::Good));
        assert_eq!(Rating::from_u8(0), None);
    }

    /// Parity against py-fsrs 6.3.2 (PyPI `fsrs==6.3.2`, FSRS-6, identical
    /// DEFAULT_PARAMETERS, `enable_fuzzing=False`). Generated 2026-09-25 by
    /// driving `Scheduler.review_card` over `tests/test_basic.py::TEST_RATINGS_1`
    /// (`test_review_card`): G,G,G,G,G,G,A,A,G,G,G,G,G reviewed exactly at each
    /// due date. py-fsrs 4.x is FSRS-5 (19 weights) and is not this trajectory;
    /// FSRS-6 lives in py-fsrs 6.x — pin 6.3.2 so drift is explicit.
    ///
    /// Same-day learning/relearning steps are 10 minutes (`0.00694444` days).
    /// Mohawk's phase machine is a deliberate simplification of py-fsrs's
    /// learning-steps scheduler, so *intervals* are not expected to match —
    /// memory-state (S/D) trajectories are the parity contract.
    #[test]
    fn parity_with_py_fsrs_reference_trajectory() {
        // (rating, elapsed_days_at_review, expected S, expected D)
        let cases: &[(u8, f64, f64, f64)] = &[
            (3, 0.0, 2.3065, 2.118104),
            (3, 0.00694444, 2.3065, 2.111214),
            (3, 2.0, 10.971048, 2.104331),
            (3, 11.0, 46.316858, 2.097455),
            (3, 46.0, 162.999816, 2.090586),
            (3, 163.0, 497.876555, 2.083724),
            (1, 498.0, 6.890413, 7.383202),
            (1, 0.00694444, 2.154598, 9.125105),
            (3, 0.00694444, 2.154598, 9.111208),
            (3, 2.0, 3.983123, 9.097325),
            (3, 4.0, 7.236254, 9.083456),
            (3, 7.0, 12.483044, 9.069601),
            (3, 12.0, 20.770357, 9.05576),
        ];

        let params = FsrsParams::default();
        let mut state = MemoryState {
            stability: 0.0,
            difficulty: 0.0,
        };
        for (i, (rating, elapsed, exp_s, exp_d)) in cases.iter().enumerate() {
            state = step(
                state,
                *elapsed,
                Rating::from_u8(*rating).unwrap(),
                i == 0,
                &params,
            );
            assert!(
                (state.stability - exp_s).abs() < 1e-4,
                "case {i}: parity: S = {}, expected {exp_s}",
                state.stability
            );
            assert!(
                (state.difficulty - exp_d).abs() < 1e-4,
                "case {i}: parity: D = {}, expected {exp_d}",
                state.difficulty
            );
        }
    }
}
