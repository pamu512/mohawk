import { invoke as tauriInvoke } from "@tauri-apps/api/core";

const TAURI_REQUIRED_MSG =
  "Tauri IPC unavailable — use the Mohawk desktop window from `npm run dev`, not http://localhost:1420 in a browser tab.";

function ipcAvailable(): boolean {
  const internals = (window as Window & { __TAURI_INTERNALS__?: { invoke?: unknown } })
    .__TAURI_INTERNALS__;
  // ponytail: devUrl loads localhost; __TAURI_INTERNALS__ is the reliable signal, not isTauri()
  return typeof internals?.invoke === "function";
}

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!ipcAvailable()) {
    throw new Error(TAURI_REQUIRED_MSG);
  }
  return tauriInvoke<T>(cmd, args);
}

export function isDesktopShell(): boolean {
  return ipcAvailable();
}

export const TAURI_IPC_ERROR = TAURI_REQUIRED_MSG;

// ==========================================
// Strongly-Typed Interfaces (Matches Rust Core)
// ==========================================

export type CardType = 'payload_drill' | 'logic_sandbox' | 'recall';

export interface Card {
  id: string;
  card_type: CardType;
  category: string | null;
  difficulty_tier: string | null;
  created_at: string | null;
  data: Record<string, unknown>;
}

export interface FsrsState {
  card_id: string;
  stability: number;
  difficulty: number;
  lapses: number;
  reviews: number;
  state: number;
  last_review: string | null;
  next_review: string;
}

export interface GraphNode {
  id: string;
  entity_type: 'vector' | 'indicator' | 'legal' | 'pattern';
  title: string;
  description: string;
}

export interface GraphEdge {
  source_node_id: string;
  target_node_id: string;
  relationship_type: string;
}

export interface KnowledgeGraph {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export interface SandboxResult {
  execution_time_us: number;
  execution_time_ms: number;
  total_evaluated: number;
  true_positives: number;
  false_positives: number;
  false_negatives: number;
  rules_triggered: number;
  fraud_caught_percentage: number;
  false_positive_percentage: number;
  syntax_valid: boolean;
  validation_errors: string[];
  challenge_language: 'SQL' | 'Python' | 'R' | 'Stats' | null;
}

export type ChallengeLanguage = 'SQL' | 'Python' | 'R' | 'Stats';

export type InferenceBackend = 'ollama_chat' | 'ollama_generate';

export interface LocalInferenceConfig {
  host: string;
  port: number;
  model: string;
  backend: InferenceBackend;
}

export interface SavedGeneratedCard {
  id: string;
  card_type: string;
  data: Record<string, unknown>;
}

export interface GenerateCardsResponse {
  cards: SavedGeneratedCard[];
}

export interface LinkedCardSummary {
  id: string;
  card_type: string;
  question: string | null;
}

export interface IngestionSource {
  id: string;
  label: string;
  url: string;
  desk_category: string;
}

export interface SyncLogLine {
  timestamp: string;
  text: string;
}

export interface SyncReportSummary {
  synced_at: string;
  feeds_attempted: number;
  feeds_succeeded: number;
  items_extracted: number;
  chunks_produced: number;
  error_count: number;
}

export interface SyncDashboardStatus {
  last_sync_at: string | null;
  next_sync_at: string;
  seconds_until_next_sync: number;
  sync_interval_days: number;
  sync_is_due: boolean;
  sync_in_progress: boolean;
  sources: IngestionSource[];
  logs: SyncLogLine[];
  last_report: SyncReportSummary | null;
  ollama: OllamaHealth;
  pending_courses: PendingCourseSummary[];
}

export interface OllamaHealth {
  online: boolean;
  model_count: number;
  default_model: string;
}

export interface PendingCourseSummary {
  id: string;
  course_title: string;
  category: string;
  source_title: string;
  chunk_index: number;
  node_count: number;
  edge_count: number;
  card_count: number;
  staged_at: string;
}

export interface PersistedCourse {
  course_title: string;
  category: string;
  node_ids: string[];
  card_ids: string[];
}

export interface ExtractedNode {
  id: string;
  entity_type: string;
  title: string;
  description: string;
}

export interface ExtractedEdge {
  source_node_id: string;
  target_node_id: string;
  relationship_type: string;
}

export interface ExtractedCard {
  card_type: string;
  question: string;
  answer: string;
  payload_mock: Record<string, unknown>;
}

export interface CourseExtraction {
  course_title: string;
  category: string;
  new_nodes: ExtractedNode[];
  new_edges: ExtractedEdge[];
  new_cards: ExtractedCard[];
}

export interface PendingCourseDetail {
  summary: PendingCourseSummary;
  course: CourseExtraction;
}

export type ExportFormat = 'csv' | 'anki_tsv';

export interface ExportResult {
  filename: string;
  content: string;
}

// ==========================================
// Core API Invocation Layer
// ==========================================

export const ApiService = {
  /**
   * Fetches the current batch of flashcards due for FSRS review.
   */
  async getNextReviewCards(limit: number = 20): Promise<Card[]> {
    try {
      return await invoke<Card[]>("get_next_review_cards", { limit });
    } catch (error) {
      console.error("Failed to fetch review cards:", error);
      throw error;
    }
  },

  /**
   * Submits an active recall score (1-4) for a card to recalculate FSRS intervals.
   */
  async submitReviewScore(cardId: string, rating: number): Promise<FsrsState> {
    try {
      return await invoke<FsrsState>("submit_review_score", {
        card_id: cardId,
        score: rating,
      });
    } catch (error) {
      console.error(`Failed to submit review score for card ${cardId}:`, error);
      throw error;
    }
  },

  /**
   * Retrieves the entire bi-directional fraud attack topology graph.
   */
  async getKnowledgeGraph(): Promise<KnowledgeGraph> {
    try {
      return await invoke<KnowledgeGraph>("get_knowledge_graph");
    } catch (error) {
      console.error("Failed to fetch knowledge graph:", error);
      throw error;
    }
  },

  /**
   * Evaluates custom validation logic rules against a batch of test transaction streams.
   */
  async executeSandboxRules(
    rules: string,
    payloads: string[],
    challengeLanguage?: ChallengeLanguage | null,
  ): Promise<SandboxResult> {
    try {
      return await invoke<SandboxResult>("execute_sandbox_rules", {
        rule_definition: rules,
        transactions: payloads,
        challenge_language: challengeLanguage ?? null,
      });
    } catch (error) {
      console.error("Sandbox execution engine failed:", error);
      throw error;
    }
  },

  /**
   * Triggers the local AI pipeline to parse raw text into structured flashcard sets.
   */
  async generateCardsFromText(
    sourceText: string,
    inference?: LocalInferenceConfig,
  ): Promise<GenerateCardsResponse> {
    try {
      return await invoke<GenerateCardsResponse>("generate_cards_from_text", {
        source_text: sourceText,
        inference: inference ?? null,
      });
    } catch (error) {
      console.error("Local AI card generation failed:", error);
      throw error;
    }
  },

  async getNodeLinkedCards(nodeId: string): Promise<LinkedCardSummary[]> {
    try {
      return await invoke<LinkedCardSummary[]>("get_node_linked_cards", { node_id: nodeId });
    } catch (error) {
      console.error("Failed to fetch linked cards:", error);
      throw error;
    }
  },

  async createManualCardForNode(
    nodeId: string,
    question: string,
    answer: string,
  ): Promise<SavedGeneratedCard> {
    try {
      return await invoke<SavedGeneratedCard>("create_manual_card_for_node", {
        node_id: nodeId,
        question,
        answer,
      });
    } catch (error) {
      console.error("Failed to create manual card:", error);
      throw error;
    }
  },

  async getSyncDashboardStatus(): Promise<SyncDashboardStatus> {
    try {
      return await invoke<SyncDashboardStatus>("get_sync_dashboard_status");
    } catch (error) {
      console.error("Failed to fetch sync dashboard status:", error);
      throw error;
    }
  },

  async forceSyncCurriculum(): Promise<void> {
    try {
      await invoke<void>("force_sync_curriculum");
    } catch (error) {
      console.error("Force sync failed:", error);
      throw error;
    }
  },

  async acceptPendingCourse(pendingId: string): Promise<PersistedCourse> {
    try {
      return await invoke<PersistedCourse>("accept_pending_course", { pending_id: pendingId });
    } catch (error) {
      console.error("Accept pending course failed:", error);
      throw error;
    }
  },

  async rejectPendingCourse(pendingId: string): Promise<void> {
    try {
      await invoke<void>("reject_pending_course", { pending_id: pendingId });
    } catch (error) {
      console.error("Reject pending course failed:", error);
      throw error;
    }
  },

  async getPendingCourseDetail(pendingId: string): Promise<PendingCourseDetail> {
    try {
      return await invoke<PendingCourseDetail>("get_pending_course_detail", {
        pending_id: pendingId,
      });
    } catch (error) {
      console.error("Failed to load pending course detail:", error);
      throw error;
    }
  },

  async getInferenceSettings(): Promise<LocalInferenceConfig> {
    try {
      return await invoke<LocalInferenceConfig>("get_inference_settings");
    } catch (error) {
      console.error("Failed to load inference settings:", error);
      throw error;
    }
  },

  async updateInferenceSettings(settings: LocalInferenceConfig): Promise<void> {
    try {
      await invoke<void>("update_inference_settings", { settings });
    } catch (error) {
      console.error("Failed to save inference settings:", error);
      throw error;
    }
  },

  async exportStudyCards(format: ExportFormat): Promise<ExportResult> {
    try {
      return await invoke<ExportResult>("export_study_cards", { format });
    } catch (error) {
      console.error("Export failed:", error);
      throw error;
    }
  },
};
