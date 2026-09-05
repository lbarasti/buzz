use super::*;

fn temp_base() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let base = dir.path().join("credentials");
    (dir, base)
}

#[test]
fn credential_dir_name_is_deterministic_and_collision_free() {
    let a = credential_dir_name("builtin:fizz");
    let b = credential_dir_name("builtin:fizz");
    assert_eq!(a, b);
    // `:` is replaced, and the hash suffix disambiguates ids that sanitize to
    // the same string ("builtin:fizz" vs "builtin_fizz" share "builtin_fizz").
    assert!(!a.contains(':'));
    let c = credential_dir_name("builtin_fizz");
    assert_ne!(a, c);
    assert!(a.starts_with("builtin_fizz-"));
}

#[test]
fn validate_persona_id_rejects_traversal_and_empties() {
    assert!(validate_persona_id("builtin:fizz").is_ok());
    assert!(validate_persona_id("my-agent_1").is_ok());
    assert!(validate_persona_id("").is_err());
    assert!(validate_persona_id("../escape").is_err());
    assert!(validate_persona_id("has space").is_err());
    assert!(validate_persona_id(&"x".repeat(65)).is_err());
}

#[test]
fn write_then_read_roundtrips_and_preserves_other_providers() {
    let (_dir, base) = temp_base();
    write_credential_at_base(&base, "builtin:fizz", "opencode-go", "sk-test-1").expect("write 1");
    write_credential_at_base(&base, "builtin:fizz", "anthropic", "sk-ant-test").expect("write 2");

    let path = opencode_auth_json_path_from_base(&base, "builtin:fizz").expect("auth path");
    let auth = read_opencode_auth(&path).expect("read");
    assert_eq!(
        auth.get("opencode-go"),
        Some(&serde_json::json!({ "type": "api", "key": "sk-test-1" }))
    );
    assert!(auth.contains_key("anthropic"));

    // Replacing one provider leaves the other intact.
    write_credential_at_base(&base, "builtin:fizz", "opencode-go", "sk-test-2").expect("replace");
    let auth = read_opencode_auth(&path).expect("read 2");
    assert_eq!(auth["opencode-go"]["key"], "sk-test-2");
    assert!(auth.contains_key("anthropic"));

    // Store is owner-readable only.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path).expect("meta").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn isolation_env_requires_provisioned_store() {
    let (_dir, base) = temp_base();
    // No store at all → fall back.
    assert_eq!(
        opencode_isolation_env_from_base(&base, "builtin:fizz").expect("isolation"),
        None
    );
    // Provisioned → isolation pair pointing at the persona roots.
    write_credential_at_base(&base, "builtin:fizz", "opencode-go", "sk-test").expect("write");
    let (data, config) = opencode_isolation_env_from_base(&base, "builtin:fizz")
        .expect("isolation 2")
        .expect("provisioned");
    let data_dir = opencode_data_dir_from_base(&base, "builtin:fizz").expect("data dir");
    let config_dir = opencode_config_dir_from_base(&base, "builtin:fizz").expect("config dir");
    assert_eq!(data, data_dir.display().to_string());
    assert_eq!(config, config_dir.display().to_string());
    assert!(data.ends_with("opencode-data"));
    assert!(config.ends_with("opencode-config"));
}

#[test]
fn clear_removes_only_target_provider() {
    let (_dir, base) = temp_base();
    write_credential_at_base(&base, "builtin:fizz", "opencode-go", "sk-a").expect("write a");
    write_credential_at_base(&base, "builtin:fizz", "anthropic", "sk-b").expect("write b");

    let removed = clear_credential_at_base(&base, "builtin:fizz", "opencode-go").expect("clear");
    assert!(removed);
    let path = opencode_auth_json_path_from_base(&base, "builtin:fizz").expect("auth path");
    let auth = read_opencode_auth(&path).expect("read");
    assert!(!auth.contains_key("opencode-go"));
    assert!(auth.contains_key("anthropic"));

    // Clearing again reports no-op.
    assert!(!clear_credential_at_base(&base, "builtin:fizz", "opencode-go").expect("clear 2"));
}

#[test]
fn write_rejects_invalid_provider_and_blank_key() {
    let (_dir, base) = temp_base();
    assert!(write_credential_at_base(&base, "builtin:fizz", "Bad Provider", "sk").is_err());
    assert!(write_credential_at_base(&base, "builtin:fizz", "anthropic", "   ").is_err());
    assert!(write_credential_at_base(&base, "invalid/persona", "anthropic", "sk").is_err());
}

#[test]
fn malformed_store_counts_as_provisioned() {
    let (_dir, base) = temp_base();
    let path = opencode_auth_json_path_from_base(&base, "builtin:fizz").expect("auth path");
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(&path, "not json at all").expect("write junk");
    assert!(isolation_store_provisioned(&path));
    // Malformed stores still isolate: the agent surfaces opencode's own auth
    // error rather than silently spending on the owner's key while the user
    // believes isolation is active.
    assert!(opencode_isolation_env_from_base(&base, "builtin:fizz")
        .expect("isolation")
        .is_some());
}

#[test]
fn enabled_marker_roundtrips_and_gates_isolation() {
    let (_dir, base) = temp_base();
    let app_marker = base
        .join(credential_dir_name("builtin:fizz"))
        .join(ENABLED_MARKER);
    assert!(!app_marker.exists());

    // Enabled + provisioned → isolation applies.
    write_credential_at_base(&base, "builtin:fizz", "opencode-go", "sk").expect("write");
    fs::write(&app_marker, b"1").expect("enable");
    assert!(opencode_isolation_env_from_base(&base, "builtin:fizz")
        .expect("isolation")
        .is_some());

    // Opt-in without a provisioned key → deliberate fallback.
    let marker2 = base
        .join(credential_dir_name("builtin:honey"))
        .join(ENABLED_MARKER);
    fs::create_dir_all(marker2.parent().expect("parent")).expect("mkdir");
    fs::write(&marker2, b"1").expect("enable 2");
    assert_eq!(
        opencode_isolation_env_from_base(&base, "builtin:honey").expect("isolation 2"),
        None
    );
}
