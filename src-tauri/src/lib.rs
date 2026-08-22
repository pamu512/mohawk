mod commands;
mod database;
mod domain;
mod engine;
mod errors;
mod state;

use state::AppState;
use tauri::Manager;
use tracing_subscriber::EnvFilter;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Setup/window init runs inside tao's did_finish_launching (extern "C" / nounwind).
    // Any panic there becomes panic_cannot_unwind → SIGABRT with a useless stack.
    // Print the real payload first.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        eprintln!("Mohawk panic: {info}");
        default_hook(info);
    }));

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .setup(|app| {
            // Returning Err from setup also panics inside did_finish_launching — exit instead.
            let handle = app.handle().clone();

            let app_state = match std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new()
                    .map_err(|e| errors::AppError::InternalError(e.to_string()))?;
                rt.block_on(AppState::init(&handle))
            })
            .join()
            {
                Ok(Ok(state)) => state,
                Ok(Err(e)) => {
                    eprintln!("Mohawk setup failed: {e}");
                    eprintln!(
                        "If this mentions missing/applied migrations, delete \
                         ~/Library/Application Support/com.mohawk.app/mohawk.db \
                         or rebuild so the binary includes all migrations under src-tauri/migrations."
                    );
                    std::process::exit(1);
                }
                Err(_) => {
                    eprintln!("Mohawk setup failed: database init thread panicked");
                    std::process::exit(1);
                }
            };

            app.manage(app_state);
            engine::sync_log::register_app(app.handle().clone());
            engine::scraper::spawn_startup_sync_if_due(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_next_review_cards,
            commands::submit_review_score,
            commands::get_knowledge_graph,
            commands::execute_sandbox_rules,
            commands::generate_cards_from_text,
            commands::get_node_linked_cards,
            commands::create_manual_card_for_node,
            commands::get_sync_dashboard_status,
            commands::force_sync_curriculum,
            commands::accept_pending_course,
            commands::reject_pending_course,
            commands::get_pending_course_detail,
            commands::get_inference_settings,
            commands::update_inference_settings,
            commands::export_study_cards,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
