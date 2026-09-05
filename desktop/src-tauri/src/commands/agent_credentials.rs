//! IPC surface for per-persona opencode credential provisioning.
//!
//! Backs the "custom credentials" opt-in in the agent definition dialog:
//! list/write/clear provider API keys in the persona's isolated opencode
//! auth store (see `managed_agents/credentials.rs`). Keys are accepted over
//! IPC, written to disk, and never returned; status projections expose
//! provider ids and directory paths only.

use tauri::AppHandle;

use crate::managed_agents::credentials::{
    clear_opencode_credential, credential_state, set_isolation_enabled, write_opencode_credential,
    PersonaOpencodeCredentialState,
};

/// Current provisioning state for a persona's isolated opencode store.
#[tauri::command]
pub async fn get_persona_opencode_credentials(
    app: AppHandle,
    persona_id: String,
) -> Result<PersonaOpencodeCredentialState, String> {
    tokio::task::spawn_blocking(move || credential_state(&app, &persona_id))
        .await
        .map_err(|error| format!("credential state task failed: {error}"))?
}

/// Write (or replace) one provider's API key in the persona's isolated store.
#[tauri::command]
pub async fn set_persona_opencode_credential(
    app: AppHandle,
    persona_id: String,
    provider_id: String,
    api_key: String,
) -> Result<PersonaOpencodeCredentialState, String> {
    tokio::task::spawn_blocking(move || {
        write_opencode_credential(&app, &persona_id, &provider_id, &api_key)?;
        credential_state(&app, &persona_id)
    })
    .await
    .map_err(|error| format!("credential write task failed: {error}"))?
}

/// Toggle the persona's custom-credentials opt-in (machine-local marker in
/// the credential store, never part of the persona definition).
#[tauri::command]
pub async fn set_persona_opencode_credentials_enabled(
    app: AppHandle,
    persona_id: String,
    enabled: bool,
) -> Result<PersonaOpencodeCredentialState, String> {
    tokio::task::spawn_blocking(move || {
        set_isolation_enabled(&app, &persona_id, enabled)?;
        credential_state(&app, &persona_id)
    })
    .await
    .map_err(|error| format!("credential toggle task failed: {error}"))?
}

/// Remove one provider's entry from the persona's isolated store.
#[tauri::command]
pub async fn clear_persona_opencode_credential(
    app: AppHandle,
    persona_id: String,
    provider_id: String,
) -> Result<PersonaOpencodeCredentialState, String> {
    tokio::task::spawn_blocking(move || {
        clear_opencode_credential(&app, &persona_id, &provider_id)?;
        credential_state(&app, &persona_id)
    })
    .await
    .map_err(|error| format!("credential clear task failed: {error}"))?
}
