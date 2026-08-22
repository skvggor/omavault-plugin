use crate::cli::Command;
use crate::gocryptfs::GOCRYPTFS;
use crate::paths::{detect_root, Layout};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::Path;

mod cli;
mod gocryptfs;
mod holders;
mod mounts;
mod paths;
mod scan;
#[cfg(test)]
mod test_support;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let root = match detect_root() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("{}", json!({ "ok": false, "error": error }));
            std::process::exit(1);
        }
    };
    let program =
        gocryptfs::find_in_path(GOCRYPTFS).unwrap_or_else(|| std::path::PathBuf::from(GOCRYPTFS));
    let code = run(
        &arguments,
        &root,
        &program,
        &mut std::io::stdin().lock(),
        &mut std::io::stdout(),
    );
    std::process::exit(code);
}

pub fn run(
    arguments: &[String],
    root: &Path,
    program: &Path,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> i32 {
    let command = match cli::parse(arguments) {
        Ok(command) => command,
        Err(error) => return emit_failure(output, &error),
    };
    if command == Command::Help {
        let _ = writeln!(output, "{}", cli::USAGE);
        return 0;
    }
    let layout = Layout::new(root);
    let secrets = match read_secrets(&command, input) {
        Ok(secrets) => secrets,
        Err(error) => return emit_failure(output, &error),
    };
    match dispatch(command, &layout, &secrets, program) {
        Ok(mut payload) => {
            payload["ok"] = json!(true);
            let _ = writeln!(output, "{}", payload);
            0
        }
        Err(error) => emit_failure(output, &error),
    }
}

pub fn dispatch(
    command: Command,
    layout: &Layout,
    secrets: &[String],
    program: &Path,
) -> Result<Value, String> {
    match command {
        Command::Status { limit } => status(layout, program, limit),
        Command::Init => init(layout, secrets.first().map(String::as_str), program),
        Command::Unlock { recovery_key } => unlock(
            layout,
            secrets.first().map(String::as_str),
            program,
            recovery_key,
        ),
        Command::SetPassphrase => set_passphrase(layout, secrets, program),
        Command::Lock { lazy } => lock(layout, lazy),
        Command::Restore => restore(layout),
        Command::Discard => discard(layout),
        Command::Help => Ok(json!({})),
    }
}

// Secrets always arrive on stdin, one per line, in the order the helper
// command expects them (e.g. recovery key first, then the new passphrase).
fn read_secrets(command: &Command, input: &mut dyn BufRead) -> Result<Vec<String>, String> {
    match command {
        Command::Init => Ok(vec![read_secret(input, "passphrase")?]),
        Command::Unlock { recovery_key } => Ok(vec![read_secret(
            input,
            if *recovery_key {
                "recovery key"
            } else {
                "passphrase"
            },
        )?]),
        Command::SetPassphrase => Ok(vec![
            read_secret(input, "recovery key")?,
            read_secret(input, "passphrase")?,
        ]),
        _ => Ok(Vec::new()),
    }
}

fn read_secret(input: &mut dyn BufRead, label: &str) -> Result<String, String> {
    let mut secret = String::new();
    input
        .read_line(&mut secret)
        .map_err(|error| format!("failed to read {}: {}", label, error))?;
    let trimmed = secret.trim_end_matches(['\n', '\r']);
    if trimmed.is_empty() {
        return Err(format!("{} is empty", label));
    }
    Ok(trimmed.to_string())
}

fn set_passphrase(layout: &Layout, secrets: &[String], program: &Path) -> Result<Value, String> {
    if !layout.is_initialized() {
        return Err("vault is not initialized".to_string());
    }
    let master_key = secrets
        .first()
        .ok_or_else(|| "recovery key is required".to_string())?
        .as_str();
    let passphrase = secrets
        .get(1)
        .ok_or_else(|| "new passphrase is required".to_string())?
        .as_str();
    validate_passphrase(passphrase)?;
    gocryptfs::set_passphrase(program, &layout.cipher_dir(), master_key, passphrase)?;
    Ok(json!({ "passphraseChanged": true }))
}

fn status(layout: &Layout, program: &Path, limit: usize) -> Result<Value, String> {
    let installed = program.is_file();
    let initialized = layout.is_initialized();
    let unlocked = initialized && gocryptfs::is_mounted(&layout.mount_dir())?;
    let (total_bytes, file_count, files, vault_holders) = if unlocked {
        let summary = scan::scan_recent(&layout.mount_dir(), limit);
        let vault_holders = holders::scan_holders(&layout.mount_dir());
        (
            summary.total_bytes,
            summary.file_count,
            summary.files,
            vault_holders,
        )
    } else {
        (0, 0, Vec::new(), Vec::new())
    };
    Ok(json!({
        "installed": installed,
        "initialized": initialized,
        "unlocked": unlocked,
        "vaultPath": layout.cipher_dir().to_string_lossy(),
        "mountPath": layout.mount_dir().to_string_lossy(),
        "usedBytes": total_bytes,
        "fileCount": file_count,
        "files": files,
        "pendingRecovered": gocryptfs::entry_count(&layout.recovered_dir()),
        "holders": vault_holders.iter().map(|holder| json!({
            "process": holder.process,
            "openPaths": holder.open_paths,
        })).collect::<Vec<_>>(),
    }))
}

fn init(layout: &Layout, passphrase: Option<&str>, program: &Path) -> Result<Value, String> {
    if layout.is_initialized() {
        return Err("vault already initialized".to_string());
    }
    let passphrase = passphrase.ok_or_else(|| "passphrase is required".to_string())?;
    validate_passphrase(passphrase)?;
    let master_key = gocryptfs::init(program, &layout.cipher_dir(), passphrase)?;
    Ok(json!({
        "initialized": true,
        "recoveryKey": master_key,
    }))
}

fn unlock(
    layout: &Layout,
    secret: Option<&str>,
    program: &Path,
    recovery_key: bool,
) -> Result<Value, String> {
    if !layout.is_initialized() {
        return Err("vault is not initialized".to_string());
    }
    if gocryptfs::is_mounted(&layout.mount_dir())? {
        return Ok(json!({ "unlocked": true }));
    }
    let secret = secret.ok_or_else(|| {
        if recovery_key {
            "recovery key is required".to_string()
        } else {
            "passphrase is required".to_string()
        }
    })?;
    let recovered = gocryptfs::recover_stale_files(&layout.mount_dir(), &layout.recovered_dir())?;
    if recovery_key {
        gocryptfs::unlock_with_recovery_key(
            program,
            &layout.cipher_dir(),
            &layout.mount_dir(),
            secret,
        )?;
    } else {
        gocryptfs::unlock(program, &layout.cipher_dir(), &layout.mount_dir(), secret)?;
    }
    Ok(json!({ "unlocked": true, "recoveredCount": recovered }))
}

fn restore(layout: &Layout) -> Result<Value, String> {
    if !layout.is_initialized() {
        return Err("vault is not initialized".to_string());
    }
    if !gocryptfs::is_mounted(&layout.mount_dir())? {
        return Err("vault is locked".to_string());
    }
    let restored =
        gocryptfs::restore_recovered_files(&layout.recovered_dir(), &layout.mount_dir())?;
    Ok(json!({ "restored": restored }))
}

fn discard(layout: &Layout) -> Result<Value, String> {
    if !layout.is_initialized() {
        return Err("vault is not initialized".to_string());
    }
    let discarded = gocryptfs::discard_recovered_files(&layout.recovered_dir())?;
    Ok(json!({ "discarded": discarded }))
}

fn lock(layout: &Layout, lazy: bool) -> Result<Value, String> {
    if !layout.is_initialized() {
        return Err("vault is not initialized".to_string());
    }
    if !gocryptfs::is_mounted(&layout.mount_dir())? {
        return Ok(json!({ "unlocked": false, "lazy": false }));
    }
    match gocryptfs::unmount(&layout.mount_dir(), lazy) {
        Ok(()) => Ok(json!({ "unlocked": false, "lazy": lazy })),
        Err(eager_error) => {
            if lazy {
                return Err(eager_error);
            }
            // An open file or file manager keeps the mount busy; detach it
            // anyway so nothing new can reach the vault.
            gocryptfs::unmount(&layout.mount_dir(), true)?;
            Ok(json!({ "unlocked": false, "lazy": true }))
        }
    }
}

fn validate_passphrase(passphrase: &str) -> Result<(), String> {
    if passphrase.chars().count() < 8 {
        return Err("passphrase must be at least 8 characters".to_string());
    }
    Ok(())
}

fn emit_failure(output: &mut dyn Write, message: &str) -> i32 {
    let _ = writeln!(output, "{}", json!({ "ok": false, "error": message }));
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fake_gocryptfs, ACCEPTED_PASSPHRASE, MASTER_KEY};
    use std::fs;
    use std::io::Cursor;
    use tempfile::TempDir;

    fn vault_layout() -> (TempDir, Layout) {
        let directory = TempDir::new().unwrap();
        let layout = Layout::new(directory.path());
        (directory, layout)
    }

    fn initialized_layout() -> (TempDir, Layout) {
        let (directory, layout) = vault_layout();
        fs::create_dir_all(layout.cipher_dir()).unwrap();
        fs::write(layout.cipher_dir().join("gocryptfs.conf"), "{}").unwrap();
        (directory, layout)
    }

    fn execute(arguments: &[&str], root: &Path, program: &Path, input: &str) -> (i32, String) {
        let owned_arguments: Vec<String> =
            arguments.iter().map(|value| value.to_string()).collect();
        let mut output = Vec::new();
        let code = run(
            &owned_arguments,
            root,
            program,
            &mut Cursor::new(input.to_string()),
            &mut output,
        );
        (code, String::from_utf8(output).unwrap())
    }

    #[test]
    fn rejects_short_passphrase() {
        assert!(validate_passphrase("short").is_err());
        assert!(validate_passphrase("").is_err());
    }

    #[test]
    fn accepts_long_enough_passphrase() {
        assert!(validate_passphrase("correct horse battery").is_ok());
    }

    #[test]
    fn run_prints_usage_for_help() {
        let (directory, _) = vault_layout();
        let (code, output) = execute(&["--help"], directory.path(), Path::new("gocryptfs"), "");
        assert_eq!(code, 0);
        assert!(output.contains("usage:"));
    }

    #[test]
    fn run_emits_json_error_for_bad_arguments() {
        let (directory, _) = vault_layout();
        let (code, output) = execute(&["explode"], directory.path(), Path::new("gocryptfs"), "");
        assert_eq!(code, 1);
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["ok"], json!(false));
        assert!(parsed["error"]
            .as_str()
            .unwrap()
            .contains("unknown command"));
    }

    #[test]
    fn run_reports_status_of_missing_vault() {
        let (directory, layout) = vault_layout();
        let program = fake_gocryptfs(directory.path());
        let (code, output) = execute(&["status"], directory.path(), &program, "");
        assert_eq!(code, 0);
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["ok"], json!(true));
        assert_eq!(parsed["initialized"], json!(false));
        assert_eq!(parsed["unlocked"], json!(false));
        assert_eq!(
            parsed["vaultPath"],
            json!(layout.cipher_dir().to_string_lossy().to_string())
        );
    }

    #[test]
    fn run_inits_vault_with_passphrase_from_stdin() {
        let (directory, _) = vault_layout();
        let program = fake_gocryptfs(directory.path());
        let (code, output) = execute(
            &["init"],
            directory.path(),
            &program,
            &format!("{}\n", ACCEPTED_PASSPHRASE),
        );
        assert_eq!(code, 0);
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["recoveryKey"], json!(MASTER_KEY));
    }

    #[test]
    fn run_rejects_empty_passphrase_on_stdin() {
        let (directory, _) = vault_layout();
        let program = fake_gocryptfs(directory.path());
        let (code, output) = execute(&["init"], directory.path(), &program, "\n");
        assert_eq!(code, 1);
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["error"], json!("passphrase is empty"));
    }

    #[test]
    fn run_names_the_missing_secret_after_the_unlock_mode() {
        let (directory, _) = vault_layout();
        let program = fake_gocryptfs(directory.path());
        let (code, output) = execute(
            &["unlock", "--recovery-key"],
            directory.path(),
            &program,
            "\n",
        );
        assert_eq!(code, 1);
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["error"], json!("recovery key is empty"));

        let (code, output) = execute(&["unlock"], directory.path(), &program, "\n");
        assert_eq!(code, 1);
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["error"], json!("passphrase is empty"));
    }

    #[test]
    fn run_unlocks_initialized_vault() {
        let (directory, _) = initialized_layout();
        let program = fake_gocryptfs(directory.path());
        let (code, output) = execute(
            &["unlock"],
            directory.path(),
            &program,
            &format!("{}\n", ACCEPTED_PASSPHRASE),
        );
        assert_eq!(code, 0);
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["unlocked"], json!(true));
    }

    #[test]
    fn run_locks_vault_when_not_mounted() {
        let (directory, _) = initialized_layout();
        let (code, output) = execute(&["lock"], directory.path(), Path::new("gocryptfs"), "");
        assert_eq!(code, 0);
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["unlocked"], json!(false));
    }

    #[test]
    fn run_lock_with_lazy_flag_on_initialized_vault() {
        let (directory, _) = initialized_layout();
        let (code, output) = execute(
            &["lock", "--lazy"],
            directory.path(),
            Path::new("gocryptfs"),
            "",
        );
        assert_eq!(code, 0);
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["unlocked"], json!(false));
    }

    #[test]
    fn status_reports_missing_program() {
        let (directory, layout) = vault_layout();
        let payload = dispatch(
            Command::Status { limit: 10 },
            &layout,
            &[],
            &directory.path().join("missing-gocryptfs"),
        )
        .unwrap();
        assert_eq!(payload["installed"], json!(false));
    }

    #[test]
    fn dispatch_init_creates_vault_and_returns_recovery_key() {
        let (directory, layout) = vault_layout();
        let program = fake_gocryptfs(directory.path());
        let payload = dispatch(
            Command::Init,
            &layout,
            &[ACCEPTED_PASSPHRASE.to_string()],
            &program,
        )
        .unwrap();
        assert_eq!(payload["recoveryKey"], json!(MASTER_KEY));
        assert!(layout.is_initialized());
    }

    #[test]
    fn dispatch_init_rejects_existing_vault() {
        let (directory, layout) = initialized_layout();
        let program = fake_gocryptfs(directory.path());
        let error = dispatch(
            Command::Init,
            &layout,
            &[ACCEPTED_PASSPHRASE.to_string()],
            &program,
        )
        .unwrap_err();
        assert_eq!(error, "vault already initialized");
    }

    #[test]
    fn dispatch_init_requires_passphrase() {
        let (directory, layout) = vault_layout();
        let program = fake_gocryptfs(directory.path());
        let error = dispatch(Command::Init, &layout, &[], &program).unwrap_err();
        assert_eq!(error, "passphrase is required");
    }

    #[test]
    fn dispatch_init_rejects_weak_passphrase() {
        let (directory, layout) = vault_layout();
        let program = fake_gocryptfs(directory.path());
        let error = dispatch(Command::Init, &layout, &["short".to_string()], &program).unwrap_err();
        assert_eq!(error, "passphrase must be at least 8 characters");
    }

    #[test]
    fn dispatch_init_surfaces_gocryptfs_failure() {
        let (directory, layout) = vault_layout();
        let program = fake_gocryptfs(directory.path());
        let error = dispatch(
            Command::Init,
            &layout,
            &["wrong passphrase value".to_string()],
            &program,
        )
        .unwrap_err();
        assert!(
            error.contains("Password dissimilar"),
            "unexpected error: {}",
            error
        );
    }

    #[test]
    fn dispatch_unlock_rejects_uninitialized_vault() {
        let (directory, layout) = vault_layout();
        let program = fake_gocryptfs(directory.path());
        let error = dispatch(
            Command::Unlock {
                recovery_key: false,
            },
            &layout,
            &[ACCEPTED_PASSPHRASE.to_string()],
            &program,
        )
        .unwrap_err();
        assert_eq!(error, "vault is not initialized");
    }

    #[test]
    fn dispatch_unlock_mounts_initialized_vault() {
        let (directory, layout) = initialized_layout();
        let program = fake_gocryptfs(directory.path());
        let payload = dispatch(
            Command::Unlock {
                recovery_key: false,
            },
            &layout,
            &[ACCEPTED_PASSPHRASE.to_string()],
            &program,
        )
        .unwrap();
        assert_eq!(payload["unlocked"], json!(true));
        assert!(layout.mount_dir().is_dir());
    }

    #[test]
    fn dispatch_unlock_with_recovery_key_mounts_vault() {
        let (directory, layout) = initialized_layout();
        let program = fake_gocryptfs(directory.path());
        let payload = dispatch(
            Command::Unlock { recovery_key: true },
            &layout,
            &[MASTER_KEY.to_string()],
            &program,
        )
        .unwrap();
        assert_eq!(payload["unlocked"], json!(true));
    }

    #[test]
    fn dispatch_unlock_rejects_wrong_recovery_key() {
        let (directory, layout) = initialized_layout();
        let program = fake_gocryptfs(directory.path());
        let error = dispatch(
            Command::Unlock { recovery_key: true },
            &layout,
            &["00000000-00000000-0000".to_string()],
            &program,
        )
        .unwrap_err();
        assert!(error.contains("master key"), "unexpected error: {}", error);
        assert!(!error.contains("00000000"), "recovery key leaked in error");
    }

    #[test]
    fn dispatch_unlock_requires_recovery_key_on_stdin() {
        let (directory, layout) = initialized_layout();
        let program = fake_gocryptfs(directory.path());
        let error = dispatch(
            Command::Unlock { recovery_key: true },
            &layout,
            &[],
            &program,
        )
        .unwrap_err();
        assert_eq!(error, "recovery key is required");
    }

    #[test]
    fn run_sets_passphrase_with_secrets_from_stdin() {
        let (directory, _) = initialized_layout();
        let program = fake_gocryptfs(directory.path());
        let (code, output) = execute(
            &["set-passphrase"],
            directory.path(),
            &program,
            &format!("{}\n{}\n", MASTER_KEY, ACCEPTED_PASSPHRASE),
        );
        assert_eq!(code, 0, "output: {}", output);
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["passphraseChanged"], json!(true));
    }

    #[test]
    fn run_set_passphrase_rejects_weak_new_passphrase() {
        let (directory, _) = initialized_layout();
        let program = fake_gocryptfs(directory.path());
        let (code, output) = execute(
            &["set-passphrase"],
            directory.path(),
            &program,
            &format!("{}\nshort\n", MASTER_KEY),
        );
        assert_eq!(code, 1);
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(
            parsed["error"],
            json!("passphrase must be at least 8 characters")
        );
    }

    #[test]
    fn dispatch_set_passphrase_requires_recovery_key() {
        let (directory, layout) = initialized_layout();
        let program = fake_gocryptfs(directory.path());
        let error = dispatch(Command::SetPassphrase, &layout, &[], &program).unwrap_err();
        assert_eq!(error, "recovery key is required");
    }

    #[test]
    fn dispatch_set_passphrase_rejects_uninitialized_vault() {
        let (_directory, layout) = vault_layout();
        let error =
            dispatch(Command::SetPassphrase, &layout, &[], Path::new("gocryptfs")).unwrap_err();
        assert_eq!(error, "vault is not initialized");
    }

    #[test]
    fn dispatch_set_passphrase_redacts_recovery_key_on_failure() {
        let (directory, layout) = initialized_layout();
        let program = fake_gocryptfs(directory.path());
        let error = dispatch(
            Command::SetPassphrase,
            &layout,
            &[
                "00000000-00000000-000000".to_string(),
                ACCEPTED_PASSPHRASE.to_string(),
            ],
            &program,
        )
        .unwrap_err();
        assert!(error.contains("master key"), "unexpected error: {}", error);
        assert!(!error.contains("00000000"), "recovery key leaked in error");
    }

    #[test]
    fn dispatch_lock_rejects_uninitialized_vault() {
        let (_directory, layout) = vault_layout();
        let error = dispatch(
            Command::Lock { lazy: false },
            &layout,
            &[],
            Path::new("gocryptfs"),
        )
        .unwrap_err();
        assert_eq!(error, "vault is not initialized");
    }

    #[test]
    fn dispatch_lock_reports_already_locked_vault() {
        let (_directory, layout) = initialized_layout();
        let payload = dispatch(
            Command::Lock { lazy: false },
            &layout,
            &[],
            Path::new("gocryptfs"),
        )
        .unwrap();
        assert_eq!(payload["unlocked"], json!(false));
    }

    #[test]
    fn dispatch_help_returns_empty_payload() {
        let (_directory, layout) = vault_layout();
        let payload = dispatch(Command::Help, &layout, &[], Path::new("gocryptfs")).unwrap();
        assert_eq!(payload, json!({}));
    }

    #[test]
    fn status_reports_pending_recovered_files() {
        let (directory, layout) = initialized_layout();
        std::fs::create_dir_all(layout.recovered_dir()).unwrap();
        std::fs::write(layout.recovered_dir().join("dropped.txt"), b"stale").unwrap();
        let program = fake_gocryptfs(directory.path());
        let payload = dispatch(Command::Status { limit: 10 }, &layout, &[], &program).unwrap();
        assert_eq!(payload["pendingRecovered"], json!(1));
    }

    #[test]
    fn dispatch_restore_requires_unlocked_vault() {
        let (_directory, layout) = initialized_layout();
        std::fs::create_dir_all(layout.recovered_dir()).unwrap();
        std::fs::write(layout.recovered_dir().join("dropped.txt"), b"stale").unwrap();
        let error = dispatch(Command::Restore, &layout, &[], Path::new("gocryptfs")).unwrap_err();
        assert_eq!(error, "vault is locked");
        assert!(layout.recovered_dir().join("dropped.txt").is_file());
    }

    #[test]
    fn dispatch_restore_rejects_uninitialized_vault() {
        let (_directory, layout) = vault_layout();
        let error = dispatch(Command::Restore, &layout, &[], Path::new("gocryptfs")).unwrap_err();
        assert_eq!(error, "vault is not initialized");
    }

    #[test]
    fn dispatch_discard_removes_recovered_files() {
        let (_directory, layout) = initialized_layout();
        std::fs::create_dir_all(layout.recovered_dir()).unwrap();
        std::fs::write(layout.recovered_dir().join("a.txt"), b"one").unwrap();
        std::fs::create_dir(layout.recovered_dir().join("b-folder")).unwrap();
        let payload = dispatch(Command::Discard, &layout, &[], Path::new("gocryptfs")).unwrap();
        assert_eq!(payload["discarded"], json!(2));
        assert!(!layout.recovered_dir().exists());
    }

    #[test]
    fn dispatch_discard_reports_zero_when_nothing_to_remove() {
        let (_directory, layout) = initialized_layout();
        let payload = dispatch(Command::Discard, &layout, &[], Path::new("gocryptfs")).unwrap();
        assert_eq!(payload["discarded"], json!(0));
    }

    mod real_gocryptfs {
        use super::*;

        fn real_program() -> Option<std::path::PathBuf> {
            gocryptfs::find_in_path(GOCRYPTFS)
        }

        #[test]
        fn vault_lock_falls_back_to_lazy_when_mount_is_busy() {
            let Some(program) = real_program() else {
                return;
            };
            let directory = TempDir::new().unwrap();
            let root = directory.path();
            let layout = Layout::new(root);

            let (code, output) = execute(
                &["init"],
                root,
                &program,
                &format!("{}\n", ACCEPTED_PASSPHRASE),
            );
            assert_eq!(code, 0, "init failed: {}", output);
            assert!(layout.is_initialized());

            let (code, output) = execute(
                &["unlock"],
                root,
                &program,
                &format!("{}\n", ACCEPTED_PASSPHRASE),
            );
            assert_eq!(code, 0, "unlock failed: {}", output);
            assert!(gocryptfs::is_mounted(&layout.mount_dir()).unwrap());

            let (code, output) = execute(&["status"], root, &program, "");
            assert_eq!(code, 0);
            let parsed: Value = serde_json::from_str(&output).unwrap();
            assert_eq!(parsed["unlocked"], json!(true));

            fs::write(layout.mount_dir().join("secret.txt"), b"classified").unwrap();

            // Simulates a file manager holding the folder open.
            let mut holder = std::process::Command::new("sleep")
                .arg("30")
                .current_dir(layout.mount_dir())
                .spawn()
                .unwrap();

            let (code, output) = execute(&["lock"], root, &program, "");
            let _ = holder.kill();
            let _ = holder.wait();
            assert_eq!(code, 0, "lock failed: {}", output);
            let parsed: Value = serde_json::from_str(&output).unwrap();
            assert_eq!(parsed["unlocked"], json!(false));
            assert_eq!(parsed["lazy"], json!(true));
            assert!(!gocryptfs::is_mounted(&layout.mount_dir()).unwrap());

            // The detached mount point no longer exposes the vault contents.
            assert!(fs::read_dir(layout.mount_dir()).unwrap().next().is_none());
        }

        #[test]
        fn vault_lock_is_eager_when_nothing_holds_the_mount() {
            let Some(program) = real_program() else {
                return;
            };
            let directory = TempDir::new().unwrap();
            let root = directory.path();
            let layout = Layout::new(root);

            execute(
                &["init"],
                root,
                &program,
                &format!("{}\n", ACCEPTED_PASSPHRASE),
            );
            execute(
                &["unlock"],
                root,
                &program,
                &format!("{}\n", ACCEPTED_PASSPHRASE),
            );
            assert!(gocryptfs::is_mounted(&layout.mount_dir()).unwrap());

            let (code, output) = execute(&["lock"], root, &program, "");
            assert_eq!(code, 0, "lock failed: {}", output);
            let parsed: Value = serde_json::from_str(&output).unwrap();
            assert_eq!(parsed["unlocked"], json!(false));
            assert_eq!(parsed["lazy"], json!(false));
            assert!(!gocryptfs::is_mounted(&layout.mount_dir()).unwrap());
        }
    }
}
