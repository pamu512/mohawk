fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(
            tauri_build::AppManifest::new().commands(&[
                "get_next_review_cards",
                "submit_review_score",
                "get_knowledge_graph",
                "execute_sandbox_rules",
                "generate_cards_from_text",
                "get_node_linked_cards",
                "create_manual_card_for_node",
                "get_sync_dashboard_status",
                "force_sync_curriculum",
                "accept_pending_course",
                "reject_pending_course",
                "get_pending_course_detail",
                "get_inference_settings",
                "update_inference_settings",
                "export_study_cards",
            ]),
        ),
    )
    .expect("failed to run tauri build");
}
