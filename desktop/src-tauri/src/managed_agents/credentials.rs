//! Per-persona isolated opencode credential stores.
//!
//! Opt-in feature (`AgentDefinition::custom_credentials`): when a persona
//! enables custom credentials and at least one provider key has been
//! provisioned, the desktop spawns the agent's opencode harness with
//! `XDG_DATA_HOME` / `XDG_CONFIG_HOME` pointed at a persona-scoped directory.
//! opencode resolves its auth store (`opencode/auth.json`), config, and
//! session database under those roots, so the agent authenticates with its own
//! key and accrues spend separately from the owner's personal opencode
//! sessions — the same isolation the standalone `buzz-agents` launcher
//! achieves with shell-level XDG overrides.
//!
//! When the toggle is on but nothing is provisioned, the isolation env is
//! withheld and the agent falls back to the owner's default opencode
//! credentials (deliberate: opt-in must not break quick-start agents).
//!
//! Secrets live only in the isolated `auth.json` file (mode `0600`). The
//! desktop never holds a provisioned key in memory beyond the write call and
//! never logs one; list/status surfaces expose provider ids only.
//!
//! Scope note: the XDG overrides are process-wide for the spawned child tree,
//! which is why they are applied only for the opencode harness and only after
//! user env vars (they cannot be overridden from persona/agent `env_vars` —
//! both keys are in `RESERVED_ENV_KEYS`).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, Runtime};

/// Env var keys the desktop sets (after user env) to redirect opencode's
/// home. Also listed in `RESERVED_ENV_KEYS` so user-supplied env cannot
/// redirect the credential store.
pub(crate) const XDG_DATA_HOME: &str = "XDG_DATA_HOME";
pub(crate) const XDG_CONFIG_HOME: &str = "XDG_CONFIG_HOME";

/// The preset harness id whose harness consumes the isolated store.
pub(crate) const OPENCODE_RUNTIME_ID: &str = "opencode";

/// Allowed characters for a provider id inside `auth.json`. Provider ids are
/// lowercase vendor slugs (e.g. `anthropic`, `opencode-go`); validating here
/// keeps malformed ids out of the JSON and the provisioning UI.
fn is_valid_provider_id(provider_id: &str) -> bool {
    !provider_id.is_empty()
        && provider_id.len() <= 128
        && provider_id.chars().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_' || c == '.'
        })
}

/// Directory name for a persona's credential store. Persona ids are not
/// filesystem-safe (`builtin:fizz` carries a `:`), so the name is the id with
/// unsafe characters replaced, disambiguated by a SHA-256 prefix of the full
/// id. Deterministic across runs; distinct ids collide only on a SHA-256
/// prefix collision.
fn credential_dir_name(persona_id: &str) -> String {
    let sanitized: String = persona_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let digest = hex::encode(Sha256::digest(persona_id.as_bytes()));
    format!("{}-{}", sanitized, &digest[..16])
}

/// Validate a persona id before it can reach the filesystem via
/// [`credential_dir_name`]. Mirrors the slug validation personas already
/// enforce at the definition boundary; defense in depth for direct IPC calls.
fn validate_persona_id(persona_id: &str) -> Result<(), String> {
    if persona_id.is_empty() || persona_id.len() > 64 {
        return Err("invalid persona id length".to_string());
    }
    if !persona_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ':')
    {
        return Err("invalid persona id".to_string());
    }
    Ok(())
}

fn create_private_dir(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir)
        .map_err(|error| format!("failed to create {}: {error}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("failed to restrict {}: {error}", dir.display()))?;
    }
    Ok(())
}

/// The persona's credential directory under `base`:
/// `<base>/<sanitized-id>-<hash>`.
fn persona_credential_dir_from_base(base: &Path, persona_id: &str) -> Result<PathBuf, String> {
    validate_persona_id(persona_id)?;
    let dir = base.join(credential_dir_name(persona_id));
    create_private_dir(&dir)?;
    Ok(dir)
}

fn opencode_data_dir_from_base(base: &Path, persona_id: &str) -> Result<PathBuf, String> {
    Ok(persona_credential_dir_from_base(base, persona_id)?.join("opencode-data"))
}

fn opencode_config_dir_from_base(base: &Path, persona_id: &str) -> Result<PathBuf, String> {
    Ok(persona_credential_dir_from_base(base, persona_id)?.join("opencode-config"))
}

/// Path of the isolated opencode auth store:
/// `<data root>/opencode/auth.json` — the same relative location
/// `XDG_DATA_HOME` produces for a stock opencode install.
fn opencode_auth_json_path_from_base(base: &Path, persona_id: &str) -> Result<PathBuf, String> {
    Ok(opencode_data_dir_from_base(base, persona_id)?
        .join("opencode")
        .join("auth.json"))
}

/// Parse the auth store into its provider map. A missing file is an empty
/// store; a malformed file is surfaced as an error so callers can distinguish
/// "not provisioned" from "unreadable".
fn read_opencode_auth(path: &Path) -> Result<BTreeMap<String, serde_json::Value>, String> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(format!("failed to read {}: {error}", path.display())),
    };
    serde_json::from_str::<BTreeMap<String, serde_json::Value>>(&content)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))
}

/// True when the isolated store holds at least one provisioned provider.
/// A malformed store counts as provisioned for spawn decisions — the agent
/// should surface opencode's own auth error rather than silently falling back
/// to the owner's key while the user believes isolation is active.
fn isolation_store_provisioned(path: &Path) -> bool {
    match read_opencode_auth(path) {
        Ok(auth) => !auth.is_empty(),
        Err(_) => true,
    }
}

/// The XDG isolation pair for a persona, when the store is provisioned.
/// `Ok(None)` means "fall back to the default opencode credentials".
fn opencode_isolation_env_from_base(
    base: &Path,
    persona_id: &str,
) -> Result<Option<(String, String)>, String> {
    let auth_path = opencode_auth_json_path_from_base(base, persona_id)?;
    if !isolation_store_provisioned(&auth_path) {
        return Ok(None);
    }
    Ok(Some((
        opencode_data_dir_from_base(base, persona_id)?
            .display()
            .to_string(),
        opencode_config_dir_from_base(base, persona_id)?
            .display()
            .to_string(),
    )))
}

/// Persist the auth map with `0600` permissions via a temp file + rename so a
/// crash mid-write cannot leave a truncated store behind.
fn write_auth_atomic(
    path: &Path,
    auth: &BTreeMap<String, serde_json::Value>,
) -> Result<(), String> {
    let serialized = serde_json::to_string_pretty(auth)
        .map_err(|error| format!("failed to serialize auth store: {error}"))?;
    let tmp_path = path.with_extension("json.tmp");
    {
        use std::io::Write;
        let mut file = fs::File::create(&tmp_path)
            .map_err(|error| format!("failed to create {}: {error}", tmp_path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600))
                .map_err(|error| format!("failed to restrict {}: {error}", tmp_path.display()))?;
        }
        file.write_all(serialized.as_bytes())
            .and_then(|()| file.flush())
            .map_err(|error| format!("failed to write {}: {error}", tmp_path.display()))?;
    }
    fs::rename(&tmp_path, path)
        .map_err(|error| format!("failed to finalize {}: {error}", path.display()))?;
    Ok(())
}

/// Marker file recording that the persona opted into custom credentials.
/// Machine-local by design: like the provisioned keys, the opt-in is
/// provisioning state for THIS machine, never part of the (shareable)
/// persona definition.
const ENABLED_MARKER: &str = "enabled";

/// Read-only check of the opt-in marker. No directory creation on the read
/// path — spawn calls this for every opencode agent.
pub(crate) fn isolation_enabled<R: Runtime>(app: &AppHandle<R>, persona_id: &str) -> bool {
    let Ok(base) = credentials_base_dir(app) else {
        return false;
    };
    base.join(credential_dir_name(persona_id))
        .join(ENABLED_MARKER)
        .is_file()
}

/// Persist or clear the opt-in marker.
pub(crate) fn set_isolation_enabled<R: Runtime>(
    app: &AppHandle<R>,
    persona_id: &str,
    enabled: bool,
) -> Result<(), String> {
    let marker = persona_credential_dir_from_base(&credentials_base_dir(app)?, persona_id)?
        .join(ENABLED_MARKER);
    if enabled {
        fs::write(&marker, b"1")
            .map_err(|error| format!("failed to write {}: {error}", marker.display()))?;
    } else {
        match fs::remove_file(&marker) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("failed to remove {}: {error}", marker.display())),
        }
    }
    Ok(())
}

/// Spawn-time credential isolation for the opencode harness. Applies the XDG
/// pair when the persona opted in AND provisioned at least one key; logs the
/// deliberate fallback otherwise. Called from `spawn_agent_child` after the
/// user-env loop so user env cannot redirect the credential store (both keys
/// are also reserved).
pub(crate) fn apply_spawn_isolation<R: Runtime>(
    app: &AppHandle<R>,
    command: &mut std::process::Command,
    record: &super::types::ManagedAgentRecord,
    effective_command: &str,
) {
    let is_opencode = super::canonical_harness_command(effective_command).as_deref()
        == super::command_for_runtime_id(OPENCODE_RUNTIME_ID).as_deref();
    if !is_opencode {
        return;
    }
    let Some(persona_id) = record.persona_id.as_deref() else {
        return;
    };
    if !isolation_enabled(app, persona_id) {
        return;
    }
    match opencode_isolation_env(app, persona_id) {
        Ok(Some((data_home, config_home))) => {
            command.env(XDG_DATA_HOME, data_home);
            command.env(XDG_CONFIG_HOME, config_home);
        }
        Ok(None) => {
            eprintln!(
                "buzz-desktop: agent {} opted into custom credentials but none are provisioned — falling back to default opencode credentials",
                record.name
            );
        }
        Err(error) => {
            eprintln!(
                "buzz-desktop: agent {} credential isolation unavailable: {error}",
                record.name
            );
        }
    }
}

/// Status projection for the provisioning UI. Provider ids only — never keys.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaOpencodeCredentialState {
    pub enabled: bool,
    pub provisioned: bool,
    pub providers: Vec<String>,
    pub data_dir: String,
    pub config_dir: String,
}

// ── AppHandle wrappers ───────────────────────────────────────────────────────

fn credentials_base_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data dir: {error}"))?
        .join("agents")
        .join("credentials");
    create_private_dir(&dir)?;
    Ok(dir)
}

/// The isolated opencode data root for a persona (becomes `XDG_DATA_HOME`).
pub(crate) fn opencode_data_dir<R: Runtime>(
    app: &AppHandle<R>,
    persona_id: &str,
) -> Result<PathBuf, String> {
    opencode_data_dir_from_base(&credentials_base_dir(app)?, persona_id)
}

/// The isolated opencode config root for a persona (becomes `XDG_CONFIG_HOME`).
pub(crate) fn opencode_config_dir<R: Runtime>(
    app: &AppHandle<R>,
    persona_id: &str,
) -> Result<PathBuf, String> {
    opencode_config_dir_from_base(&credentials_base_dir(app)?, persona_id)
}

/// Path of the isolated opencode auth store for a persona.
pub(crate) fn opencode_auth_json_path<R: Runtime>(
    app: &AppHandle<R>,
    persona_id: &str,
) -> Result<PathBuf, String> {
    opencode_auth_json_path_from_base(&credentials_base_dir(app)?, persona_id)
}

/// The XDG isolation pair for a persona, when the store is provisioned.
pub(crate) fn opencode_isolation_env<R: Runtime>(
    app: &AppHandle<R>,
    persona_id: &str,
) -> Result<Option<(String, String)>, String> {
    opencode_isolation_env_from_base(&credentials_base_dir(app)?, persona_id)
}

/// Provider ids present in the isolated store, sorted.
pub(crate) fn list_opencode_credential_providers<R: Runtime>(
    app: &AppHandle<R>,
    persona_id: &str,
) -> Result<Vec<String>, String> {
    let path = opencode_auth_json_path(app, persona_id)?;
    Ok(read_opencode_auth(&path)?.into_keys().collect())
}

/// Write (or replace) one provider's API key in the isolated store. The entry
/// shape matches what `opencode auth login` persists for API-key providers:
/// `{ "<provider>": { "type": "api", "key": "..." } }`. Existing entries for
/// other providers are preserved.
pub(crate) fn write_opencode_credential<R: Runtime>(
    app: &AppHandle<R>,
    persona_id: &str,
    provider_id: &str,
    api_key: &str,
) -> Result<(), String> {
    write_credential_at_base(
        &credentials_base_dir(app)?,
        persona_id,
        provider_id,
        api_key,
    )
}

/// Path-based core of [`write_opencode_credential`], shared with tests.
pub(crate) fn write_credential_at_base(
    base: &Path,
    persona_id: &str,
    provider_id: &str,
    api_key: &str,
) -> Result<(), String> {
    if !is_valid_provider_id(provider_id) {
        return Err(format!("invalid provider id: {provider_id}"));
    }
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Err("API key must not be empty".to_string());
    }
    let path = opencode_auth_json_path_from_base(base, persona_id)?;
    if let Some(parent) = path.parent() {
        create_private_dir(parent)?;
    }
    let mut auth = read_opencode_auth(&path)?;
    auth.insert(
        provider_id.to_string(),
        serde_json::json!({ "type": "api", "key": api_key }),
    );
    write_auth_atomic(&path, &auth)
}

/// Remove one provider's entry from the isolated store. Returns `true` when an
/// entry was removed.
pub(crate) fn clear_opencode_credential<R: Runtime>(
    app: &AppHandle<R>,
    persona_id: &str,
    provider_id: &str,
) -> Result<bool, String> {
    clear_credential_at_base(&credentials_base_dir(app)?, persona_id, provider_id)
}

/// Path-based core of [`clear_opencode_credential`], shared with tests.
pub(crate) fn clear_credential_at_base(
    base: &Path,
    persona_id: &str,
    provider_id: &str,
) -> Result<bool, String> {
    let path = opencode_auth_json_path_from_base(base, persona_id)?;
    let mut auth = read_opencode_auth(&path)?;
    if auth.remove(provider_id).is_none() {
        return Ok(false);
    }
    write_auth_atomic(&path, &auth)?;
    Ok(true)
}

/// Build the status projection for the provisioning UI.
pub(crate) fn credential_state<R: Runtime>(
    app: &AppHandle<R>,
    persona_id: &str,
) -> Result<PersonaOpencodeCredentialState, String> {
    let providers = list_opencode_credential_providers(app, persona_id)?;
    Ok(PersonaOpencodeCredentialState {
        enabled: isolation_enabled(app, persona_id),
        provisioned: !providers.is_empty(),
        providers,
        data_dir: opencode_data_dir(app, persona_id)?.display().to_string(),
        config_dir: opencode_config_dir(app, persona_id)?.display().to_string(),
    })
}

/// Merge the isolation pair into a model-discovery env map when the persona
/// opted in AND provisioned credentials — the same gate `apply_spawn_isolation`
/// applies at spawn, so discovery always agrees with what the agent can
/// authenticate with. Returns `true` when isolation was applied.
pub(crate) fn extend_env_with_isolation<R: Runtime>(
    env: &mut BTreeMap<String, String>,
    app: &AppHandle<R>,
    persona_id: &str,
) -> bool {
    if !isolation_enabled(app, persona_id) {
        return false;
    }
    match opencode_isolation_env(app, persona_id) {
        Ok(Some((data_home, config_home))) => {
            env.insert(XDG_DATA_HOME.to_string(), data_home);
            env.insert(XDG_CONFIG_HOME.to_string(), config_home);
            true
        }
        Ok(None) => false,
        Err(error) => {
            eprintln!("buzz-desktop: opencode credential isolation unavailable: {error}");
            false
        }
    }
}

#[cfg(test)]
mod tests;
