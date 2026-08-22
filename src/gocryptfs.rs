use crate::mounts::is_mounted_text;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const GOCRYPTFS: &str = "gocryptfs";

pub fn find_in_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var("PATH").ok()?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
}

pub fn strip_ansi(text: &str) -> String {
    let mut cleaned = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '\u{1b}' {
            cleaned.push(character);
            continue;
        }
        // Skip CSI sequences: ESC [ <parameters> <final letter>.
        if characters.peek() == Some(&'[') {
            characters.next();
            for trailing in characters.by_ref() {
                if trailing.is_ascii_alphabetic() {
                    break;
                }
            }
        }
    }
    cleaned
}

pub fn extract_master_key(output: &str) -> Option<String> {
    let cleaned = strip_ansi(output);
    let after_marker = cleaned.split_once("master key")?.1;
    let compact: String = after_marker
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    let start = compact.find(|character: char| character.is_ascii_hexdigit())?;
    let end = compact[start..]
        .find(|character: char| !character.is_ascii_hexdigit() && character != '-')
        .map(|offset| start + offset)
        .unwrap_or(compact.len());
    let token = compact[start..end].trim_end_matches('-');
    if is_master_key_token(token) {
        Some(token.to_string())
    } else {
        None
    }
}

fn is_master_key_token(token: &str) -> bool {
    if token.ends_with('-') || token.starts_with('-') {
        return false;
    }
    let groups: Vec<&str> = token.split('-').collect();
    if groups.len() < 4 {
        return false;
    }
    let group_length = groups[0].len();
    if group_length != 4 && group_length != 8 {
        return false;
    }
    let hex_count: usize = groups.iter().map(|group| group.len()).sum();
    groups.iter().all(|group| {
        group.len() == group_length && group.bytes().all(|byte| byte.is_ascii_hexdigit())
    }) && hex_count >= 52
}

const TEXT_FILE_BUSY: i32 = 26;
const BUSY_RETRY_ATTEMPTS: usize = 10;
const BUSY_RETRY_DELAY_MS: u64 = 20;

// Executing a program right after writing it can fail with ETXTBSY while the
// kernel finishes flushing the file; retry briefly.
fn with_busy_retry<T>(
    attempt: &mut dyn FnMut() -> Result<T, std::io::Error>,
) -> Result<T, std::io::Error> {
    let mut attempts = 0;
    loop {
        match attempt() {
            Ok(value) => return Ok(value),
            Err(error) => {
                attempts += 1;
                if error.raw_os_error() != Some(TEXT_FILE_BUSY) || attempts >= BUSY_RETRY_ATTEMPTS {
                    return Err(error);
                }
                std::thread::sleep(std::time::Duration::from_millis(BUSY_RETRY_DELAY_MS));
            }
        }
    }
}

fn spawn_with_stdin(
    program: &Path,
    arguments: &[&str],
) -> Result<std::process::Child, std::io::Error> {
    with_busy_retry(&mut || {
        Command::new(program)
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
    })
}

fn output_with_retry(
    build_command: &mut dyn FnMut() -> Command,
) -> Result<std::process::Output, std::io::Error> {
    with_busy_retry(&mut || build_command().output())
}

fn run_with_passphrase(
    program: &Path,
    arguments: &[&str],
    passphrase_lines: &str,
) -> Result<std::process::Output, String> {
    let mut child = spawn_with_stdin(program, arguments)
        .map_err(|error| format!("failed to start {}: {}", program.display(), error))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "failed to open stdin of gocryptfs".to_string())?
        .write_all(passphrase_lines.as_bytes())
        .map_err(|error| format!("failed to write passphrase: {}", error))?;
    child
        .wait_with_output()
        .map_err(|error| format!("failed to wait for gocryptfs: {}", error))
}

fn describe_failure(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout.trim_end(), stderr.trim_end())
        .trim()
        .to_string();
    if combined.is_empty() {
        format!(
            "gocryptfs exited with status {}",
            output.status.code().unwrap_or(-1)
        )
    } else {
        combined
    }
}

// The pseudo-tty used by init echoes typed input back, so a failure message
// can contain the raw passphrase; scrub it before surfacing the error.
fn redact_secret(message: &str, secret: &str) -> String {
    if secret.is_empty() {
        return message.to_string();
    }
    message.replace(secret, "***")
}

pub fn init(program: &Path, cipher_dir: &Path, passphrase: &str) -> Result<String, String> {
    std::fs::create_dir_all(cipher_dir)
        .map_err(|error| format!("failed to create {}: {}", cipher_dir.display(), error))?;
    // gocryptfs only prints the master key on a terminal, so run it under a
    // pseudo-tty via util-linux `script`.
    let script = find_in_path("script")
        .ok_or_else(|| "util-linux script is required to display the master key".to_string())?;
    let quoted_dir = format!("'{}'", cipher_dir.to_string_lossy().replace('\'', "'\\''"));
    let inner_command = format!("{} -init {}", program.display(), quoted_dir);
    let output = run_with_passphrase(
        &script,
        &["-qec", &inner_command, "/dev/null"],
        &format!("{}\n{}\n", passphrase, passphrase),
    )?;
    if !output.status.success() {
        return Err(redact_secret(&describe_failure(&output), passphrase));
    }
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    extract_master_key(&combined).ok_or_else(|| {
        "vault was created but the master key could not be read from gocryptfs output".to_string()
    })
}

pub fn unlock(
    program: &Path,
    cipher_dir: &Path,
    mount_dir: &Path,
    passphrase: &str,
) -> Result<(), String> {
    std::fs::create_dir_all(mount_dir)
        .map_err(|error| format!("failed to create {}: {}", mount_dir.display(), error))?;
    let output = run_with_passphrase(
        program,
        &[
            "-q",
            &cipher_dir.to_string_lossy(),
            &mount_dir.to_string_lossy(),
        ],
        &format!("{}\n", passphrase),
    )?;
    if output.status.success() {
        Ok(())
    } else {
        Err(redact_secret(&describe_failure(&output), passphrase))
    }
}

// gocryptfs only accepts the explicit master key as a command-line argument
// (stdin does not work for it), so it briefly appears in /proc/<pid>/cmdline
// of the child. The key always enters this process through stdin and is
// scrubbed from any surfaced error.
pub fn unlock_with_recovery_key(
    program: &Path,
    cipher_dir: &Path,
    mount_dir: &Path,
    master_key: &str,
) -> Result<(), String> {
    std::fs::create_dir_all(mount_dir)
        .map_err(|error| format!("failed to create {}: {}", mount_dir.display(), error))?;
    let output = run_with_passphrase(
        program,
        &[
            "-q",
            &format!("-masterkey={}", master_key),
            &cipher_dir.to_string_lossy(),
            &mount_dir.to_string_lossy(),
        ],
        "",
    )?;
    if output.status.success() {
        Ok(())
    } else {
        Err(redact_secret(&describe_failure(&output), master_key))
    }
}

// Re-wraps the master key with a new passphrase. Like the recovery unlock,
// the master key only enters as a command-line argument because gocryptfs
// does not accept it on stdin; it is scrubbed from any surfaced error.
pub fn set_passphrase(
    program: &Path,
    cipher_dir: &Path,
    master_key: &str,
    new_passphrase: &str,
) -> Result<(), String> {
    let output = run_with_passphrase(
        program,
        &[
            "-passwd",
            &format!("-masterkey={}", master_key),
            &cipher_dir.to_string_lossy(),
        ],
        &format!("{}\n{}\n", new_passphrase, new_passphrase),
    )?;
    if output.status.success() {
        Ok(())
    } else {
        let failure = describe_failure(&output);
        Err(redact_secret(
            &redact_secret(&failure, master_key),
            new_passphrase,
        ))
    }
}

// Files saved into the mountpoint while the vault was locked (e.g. dropped
// via a stale file manager window) block the next mount. Move them aside
// instead of failing the unlock.
pub fn recover_stale_files(mount_dir: &Path, recovered_dir: &Path) -> Result<usize, String> {
    let entries: Vec<_> = match std::fs::read_dir(mount_dir) {
        Ok(entries) => entries.flatten().collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(format!("failed to read {}: {}", mount_dir.display(), error)),
    };
    if entries.is_empty() {
        return Ok(0);
    }
    std::fs::create_dir_all(recovered_dir)
        .map_err(|error| format!("failed to create {}: {}", recovered_dir.display(), error))?;
    let mut moved = 0;
    for entry in entries {
        let destination = unique_destination(recovered_dir, entry.file_name());
        std::fs::rename(entry.path(), &destination).map_err(|error| {
            format!(
                "failed to move {} to {}: {}",
                entry.path().display(),
                destination.display(),
                error
            )
        })?;
        moved += 1;
    }
    Ok(moved)
}

pub fn entry_count(directory: &Path) -> usize {
    std::fs::read_dir(directory)
        .map(|entries| entries.flatten().count())
        .unwrap_or(0)
}

// Recovered files sit unencrypted outside the vault; restoring moves them
// back in so they are encrypted again. The vault is a FUSE mount, so a plain
// rename usually fails with EXDEV and a copy+delete fallback is required.
pub fn restore_recovered_files(recovered_dir: &Path, mount_dir: &Path) -> Result<usize, String> {
    let entries: Vec<_> = match std::fs::read_dir(recovered_dir) {
        Ok(entries) => entries.flatten().collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            return Err(format!(
                "failed to read {}: {}",
                recovered_dir.display(),
                error
            ))
        }
    };
    if entries.is_empty() {
        return Ok(0);
    }
    std::fs::create_dir_all(mount_dir)
        .map_err(|error| format!("failed to create {}: {}", mount_dir.display(), error))?;
    let mut restored = 0;
    for entry in entries {
        let destination = unique_destination(mount_dir, entry.file_name());
        move_entry(&entry.path(), &destination)?;
        restored += 1;
    }
    // Leave no empty 'recovered' folder behind once everything was moved.
    let _ = std::fs::remove_dir(recovered_dir);
    Ok(restored)
}

pub fn discard_recovered_files(recovered_dir: &Path) -> Result<usize, String> {
    let entries: Vec<_> = match std::fs::read_dir(recovered_dir) {
        Ok(entries) => entries.flatten().collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            return Err(format!(
                "failed to read {}: {}",
                recovered_dir.display(),
                error
            ))
        }
    };
    let mut discarded = 0;
    for entry in entries {
        remove_path(&entry.path())?;
        discarded += 1;
    }
    let _ = std::fs::remove_dir(recovered_dir);
    Ok(discarded)
}

fn unique_destination(parent: &Path, file_name: std::ffi::OsString) -> PathBuf {
    let mut destination = parent.join(&file_name);
    let mut suffix = 1;
    while destination.exists() {
        destination = parent.join(format!("{}-{}", file_name.to_string_lossy(), suffix));
        suffix += 1;
    }
    destination
}

const EXDEV: i32 = 18;

fn move_entry(source: &Path, destination: &Path) -> Result<(), String> {
    match std::fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(error) if error.raw_os_error() == Some(EXDEV) => {
            copy_recursively(source, destination)?;
            remove_path(source)
        }
        Err(error) => Err(format!(
            "failed to move {} to {}: {}",
            source.display(),
            destination.display(),
            error
        )),
    }
}

fn copy_recursively(source: &Path, destination: &Path) -> Result<(), String> {
    if source.is_symlink() {
        let target = std::fs::read_link(source)
            .map_err(|error| format!("failed to read symlink {}: {}", source.display(), error))?;
        std::os::unix::fs::symlink(&target, destination).map_err(|error| {
            format!(
                "failed to create symlink {}: {}",
                destination.display(),
                error
            )
        })?;
        return Ok(());
    }
    if source.is_dir() {
        std::fs::create_dir_all(destination)
            .map_err(|error| format!("failed to create {}: {}", destination.display(), error))?;
        for entry in std::fs::read_dir(source)
            .map_err(|error| format!("failed to read {}: {}", source.display(), error))?
            .flatten()
        {
            copy_recursively(&entry.path(), &destination.join(entry.file_name()))?;
        }
        return Ok(());
    }
    std::fs::copy(source, destination).map_err(|error| {
        format!(
            "failed to copy {} to {}: {}",
            source.display(),
            destination.display(),
            error
        )
    })?;
    Ok(())
}

fn remove_path(path: &Path) -> Result<(), String> {
    let result = if path.is_dir() && !path.is_symlink() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    result.map_err(|error| format!("failed to delete {}: {}", path.display(), error))
}

pub fn unmount_candidates(mount_dir: &Path, lazy: bool) -> Vec<(String, Vec<String>)> {
    let mount_dir = mount_dir.to_string_lossy().to_string();
    if lazy {
        vec![
            (
                "fusermount3".to_string(),
                vec!["-uz".to_string(), mount_dir.clone()],
            ),
            (
                "fusermount".to_string(),
                vec!["-uz".to_string(), mount_dir.clone()],
            ),
            (
                "umount".to_string(),
                vec!["-l".to_string(), mount_dir.clone()],
            ),
        ]
    } else {
        vec![
            (
                "fusermount3".to_string(),
                vec!["-u".to_string(), mount_dir.clone()],
            ),
            (
                "fusermount".to_string(),
                vec!["-u".to_string(), mount_dir.clone()],
            ),
            ("umount".to_string(), vec![mount_dir.clone()]),
        ]
    }
}

pub fn unmount_with(candidates: &[(String, Vec<String>)]) -> Result<(), String> {
    let mut errors: Vec<String> = Vec::new();
    for (program, arguments) in candidates {
        let mut build_command = || {
            let mut command = Command::new(program);
            command.args(arguments);
            command
        };
        match output_with_retry(&mut build_command) {
            Ok(output) if output.status.success() => return Ok(()),
            Ok(output) => errors.push(describe_failure(&output)),
            Err(error) => errors.push(format!("{}: {}", program, error)),
        }
    }
    Err(format!("failed to unmount: {}", errors.join("; ")))
}

pub fn unmount(mount_dir: &Path, lazy: bool) -> Result<(), String> {
    unmount_with(&unmount_candidates(mount_dir, lazy))
}

// Fail closed: an unreadable mount table must not be reported as "locked",
// otherwise locking and auto-lock would silently skip a mounted vault.
pub fn is_mounted(mount_dir: &Path) -> Result<bool, String> {
    let content = std::fs::read_to_string("/proc/self/mounts")
        .map_err(|error| format!("failed to read mount table: {}", error))?;
    Ok(is_mounted_text(&content, &mount_dir.to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_INIT_OUTPUT: &str = "Choose a password for protecting your files.\nYour master key is:\n\n    6f2f38e6-93a3f5ac-1bd0cbb2-6ba0d9a4-ea82e9a7-77c37cd8-a5f0e13d-29f28e1d-59b21c53-8ec30a29-64bfa09e-4a29921e-d74e3ec7\n\nIf the gocryptfs.conf file is corrupted or lose the password, there will be no recovery.\n";

    #[test]
    fn extracts_master_key_from_init_output() {
        assert_eq!(
            extract_master_key(SAMPLE_INIT_OUTPUT).as_deref(),
            Some("6f2f38e6-93a3f5ac-1bd0cbb2-6ba0d9a4-ea82e9a7-77c37cd8-a5f0e13d-29f28e1d-59b21c53-8ec30a29-64bfa09e-4a29921e-d74e3ec7")
        );
    }

    #[test]
    fn returns_none_when_no_key_present() {
        assert_eq!(extract_master_key("Password:\nSome error occurred"), None);
    }

    #[test]
    fn accepts_sixteen_group_master_key() {
        let output = "Your master key is:\n\n    6f2f38e6-93a3f5ac-1bd0cbb2-6ba0d9a4-ea82e9a7-77c37cd8-a5f0e13d-29f28e1d-59b21c53-8ec30a29-64bfa09e-4a29921e-d74e3ec7-1a2b3c4d-5e6f7081-9a0b1c2d\n";
        assert!(extract_master_key(output).is_some());
    }

    #[test]
    fn rejects_token_with_wrong_length() {
        assert_eq!(extract_master_key("abcd-abcd-abcd"), None);
    }

    #[test]
    fn rejects_hex_token_without_dashes() {
        assert_eq!(extract_master_key("6f2f38e693a3f5ac1bd0cbb26ba0d9a4"), None);
    }

    #[test]
    fn rejects_token_with_trailing_dash() {
        assert_eq!(extract_master_key("6f2f38e6-93a3f5ac-1bd0cbb2-"), None);
    }

    #[test]
    fn extracts_wrapped_ansi_master_key_from_real_gocryptfs_output() {
        let output = "Password: \r\nRepeat: \r\n\r\nYour master key is:\r\n\r\n    \u{1b}[2maa4b0d5a-c464051a-84c38ba2-96e4eb91-\r\n    0f1e2d3c-4b5a6978-8796a5b4-c3d2e1f0\u{1b}[0m\r\n\r\n\u{1b}[32mThe gocryptfs filesystem has been created successfully.\u{1b}[0m\r\n";
        assert_eq!(
            extract_master_key(output).as_deref(),
            Some("aa4b0d5a-c464051a-84c38ba2-96e4eb91-0f1e2d3c-4b5a6978-8796a5b4-c3d2e1f0")
        );
    }

    #[test]
    fn extracts_key_when_password_echo_precedes_marker() {
        let output = "correct horse battery\r\ncorrect horse battery\r\nChoose a password for protecting your files.\r\nYour master key is:\r\n\r\n    6f2f38e6-93a3f5ac-1bd0cbb2-6ba0d9a4-ea82e9a7-77c37cd8-a5f0e13d-29f28e1d-59b21c53-8ec30a29-64bfa09e-4a29921e-d74e3ec7\r\n";
        assert!(extract_master_key(output).is_some());
    }

    #[test]
    fn strip_ansi_removes_csi_sequences() {
        assert_eq!(strip_ansi("\u{1b}[32mok\u{1b}[0m"), "ok");
        assert_eq!(strip_ansi("\u{1b}[2ma-b\u{1b}[0m"), "a-b");
        assert_eq!(strip_ansi("no escapes"), "no escapes");
    }

    #[test]
    fn strip_ansi_keeps_lone_escape_characters() {
        assert_eq!(strip_ansi("a\u{1b}b"), "ab");
    }

    #[test]
    fn redact_secret_scrubs_passphrase_from_pty_echo() {
        let echoed = "correct horse battery\r\nPassword dissimilar.";
        assert_eq!(
            redact_secret(echoed, "correct horse battery"),
            "***\r\nPassword dissimilar."
        );
        assert_eq!(redact_secret("no secret here", ""), "no secret here");
    }

    #[test]
    fn find_in_path_locates_system_program() {
        let found = find_in_path("sh").expect("sh should be in PATH");
        assert!(found.is_file());
    }

    #[test]
    fn find_in_path_returns_none_for_absent_program() {
        assert!(find_in_path("definitely-not-a-real-program-xyz").is_none());
    }

    mod integration {
        use super::*;
        use crate::test_support::{fake_gocryptfs, ACCEPTED_PASSPHRASE, MASTER_KEY};
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        use tempfile::TempDir;

        #[test]
        fn init_returns_master_key_and_reports_stderr_failure() {
            let directory = TempDir::new().unwrap();
            let program = fake_gocryptfs(directory.path());
            let cipher_dir = directory.path().join("vault");

            let master_key = init(&program, &cipher_dir, ACCEPTED_PASSPHRASE).unwrap();
            assert!(master_key.starts_with("6f2f38e6-"));
            assert!(cipher_dir.join("gocryptfs.conf").is_file());

            let error = init(&program, &cipher_dir, "wrong passphrase").unwrap_err();
            assert!(
                error.contains("Password dissimilar"),
                "unexpected error: {}",
                error
            );
        }

        #[test]
        fn init_fails_when_program_cannot_start() {
            let directory = TempDir::new().unwrap();
            let error = init(
                &directory.path().join("missing-gocryptfs"),
                &directory.path().join("vault"),
                ACCEPTED_PASSPHRASE,
            )
            .unwrap_err();
            assert!(!error.is_empty());
            assert!(!directory.path().join("vault/gocryptfs.conf").is_file());
        }

        #[test]
        fn init_fails_when_output_has_no_master_key() {
            let directory = TempDir::new().unwrap();
            let script = directory.path().join("silent-gocryptfs");
            fs::write(&script, "#!/bin/sh\nread _ || true\nexit 0\n").unwrap();
            fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

            let error = init(
                &script,
                &directory.path().join("vault"),
                ACCEPTED_PASSPHRASE,
            )
            .unwrap_err();
            assert!(error.contains("master key could not be read"));
        }

        #[test]
        fn unlock_succeeds_with_correct_passphrase_and_fails_otherwise() {
            let directory = TempDir::new().unwrap();
            let program = fake_gocryptfs(directory.path());
            let cipher_dir = directory.path().join("vault");
            let mount_dir = directory.path().join("mount");
            fs::create_dir_all(&cipher_dir).unwrap();

            unlock(&program, &cipher_dir, &mount_dir, ACCEPTED_PASSPHRASE).unwrap();
            assert!(mount_dir.is_dir());

            let error = unlock(&program, &cipher_dir, &mount_dir, "wrong passphrase").unwrap_err();
            assert_eq!(error, "Password incorrect.");
        }

        #[test]
        fn set_passphrase_succeeds_with_valid_master_key() {
            let directory = TempDir::new().unwrap();
            let program = fake_gocryptfs(directory.path());
            let cipher_dir = directory.path().join("vault");
            fs::create_dir_all(&cipher_dir).unwrap();

            set_passphrase(&program, &cipher_dir, MASTER_KEY, "new secret phrase").unwrap();
        }

        #[test]
        fn set_passphrase_rejects_invalid_master_key() {
            let directory = TempDir::new().unwrap();
            let program = fake_gocryptfs(directory.path());
            let cipher_dir = directory.path().join("vault");
            fs::create_dir_all(&cipher_dir).unwrap();

            let error = set_passphrase(
                &program,
                &cipher_dir,
                "11111111-22222222-33333333",
                "new secret phrase",
            )
            .unwrap_err();
            assert!(error.contains("master key"), "unexpected error: {}", error);
        }

        #[test]
        fn unlock_fails_when_program_cannot_start() {
            let directory = TempDir::new().unwrap();
            let error = unlock(
                &directory.path().join("missing-gocryptfs"),
                &directory.path().join("vault"),
                &directory.path().join("mount"),
                ACCEPTED_PASSPHRASE,
            )
            .unwrap_err();
            assert!(error.contains("failed to start"));
        }

        #[test]
        fn unmount_with_succeeds_on_first_successful_candidate() {
            let directory = TempDir::new().unwrap();
            let success = directory.path().join("always-succeeds");
            fs::write(&success, "#!/bin/sh\nexit 0\n").unwrap();
            fs::set_permissions(&success, fs::Permissions::from_mode(0o755)).unwrap();

            let candidates = vec![(success.to_string_lossy().to_string(), Vec::new())];
            unmount_with(&candidates).unwrap();
        }

        #[test]
        fn unmount_with_aggregates_candidate_failures() {
            let directory = TempDir::new().unwrap();
            let failing = directory.path().join("always-fails");
            fs::write(&failing, "#!/bin/sh\necho 'device is busy' >&2\nexit 1\n").unwrap();
            fs::set_permissions(&failing, fs::Permissions::from_mode(0o755)).unwrap();

            let candidates = vec![
                (failing.to_string_lossy().to_string(), Vec::new()),
                ("definitely-not-an-unmounter-xyz".to_string(), Vec::new()),
            ];
            let error = unmount_with(&candidates).unwrap_err();
            assert!(error.contains("device is busy"));
            assert!(error.contains("definitely-not-an-unmounter-xyz"));
        }

        #[test]
        fn recover_stale_files_moves_contents_to_recovered_dir() {
            let directory = TempDir::new().unwrap();
            let mount_dir = directory.path().join("Protected Files");
            let recovered_dir = directory.path().join("recovered");
            fs::create_dir_all(&mount_dir).unwrap();
            fs::write(mount_dir.join("stale.md"), b"dropped while locked").unwrap();
            fs::create_dir(mount_dir.join("stale-folder")).unwrap();

            let moved = recover_stale_files(&mount_dir, &recovered_dir).unwrap();
            assert_eq!(moved, 2);
            assert!(recovered_dir.join("stale.md").is_file());
            assert!(recovered_dir.join("stale-folder").is_dir());
            assert!(fs::read_dir(&mount_dir).unwrap().next().is_none());
        }

        #[test]
        fn recover_stale_files_suffices_name_collisions() {
            let directory = TempDir::new().unwrap();
            let mount_dir = directory.path().join("Protected Files");
            let recovered_dir = directory.path().join("recovered");
            fs::create_dir_all(&mount_dir).unwrap();
            fs::create_dir_all(&recovered_dir).unwrap();
            fs::write(recovered_dir.join("stale.md"), b"first").unwrap();
            fs::write(mount_dir.join("stale.md"), b"second").unwrap();

            let moved = recover_stale_files(&mount_dir, &recovered_dir).unwrap();
            assert_eq!(moved, 1);
            assert_eq!(
                fs::read_to_string(recovered_dir.join("stale.md")).unwrap(),
                "first"
            );
            assert_eq!(
                fs::read_to_string(recovered_dir.join("stale.md-1")).unwrap(),
                "second"
            );
        }

        #[test]
        fn recover_stale_files_returns_zero_for_empty_mountpoint() {
            let directory = TempDir::new().unwrap();
            let mount_dir = directory.path().join("Protected Files");
            fs::create_dir_all(&mount_dir).unwrap();

            let moved =
                recover_stale_files(&mount_dir, &directory.path().join("recovered")).unwrap();
            assert_eq!(moved, 0);
            assert!(!directory.path().join("recovered").exists());
        }

        #[test]
        fn recover_stale_files_treats_missing_mountpoint_as_empty() {
            let directory = TempDir::new().unwrap();
            let moved = recover_stale_files(
                &directory.path().join("missing"),
                &directory.path().join("recovered"),
            )
            .unwrap();
            assert_eq!(moved, 0);
        }

        #[test]
        fn restore_recovered_files_moves_items_into_destination() {
            let directory = TempDir::new().unwrap();
            let recovered = directory.path().join("recovered");
            let mount = directory.path().join("Protected Files");
            std::fs::create_dir_all(&recovered).unwrap();
            std::fs::write(recovered.join("note.md"), b"hello").unwrap();
            std::fs::create_dir(recovered.join("folder")).unwrap();
            std::fs::write(recovered.join("folder/inner.txt"), b"nested").unwrap();

            let restored = restore_recovered_files(&recovered, &mount).unwrap();
            assert_eq!(restored, 2);
            assert_eq!(
                std::fs::read_to_string(mount.join("note.md")).unwrap(),
                "hello"
            );
            assert_eq!(
                std::fs::read_to_string(mount.join("folder/inner.txt")).unwrap(),
                "nested"
            );
            assert!(!recovered.exists());
        }

        #[test]
        fn restore_recovered_files_suffices_name_collisions() {
            let directory = TempDir::new().unwrap();
            let recovered = directory.path().join("recovered");
            let mount = directory.path().join("Protected Files");
            std::fs::create_dir_all(&recovered).unwrap();
            std::fs::create_dir_all(&mount).unwrap();
            std::fs::write(recovered.join("note.md"), b"stale").unwrap();
            std::fs::write(mount.join("note.md"), b"live").unwrap();

            let restored = restore_recovered_files(&recovered, &mount).unwrap();
            assert_eq!(restored, 1);
            assert_eq!(
                std::fs::read_to_string(mount.join("note.md")).unwrap(),
                "live"
            );
            assert_eq!(
                std::fs::read_to_string(mount.join("note.md-1")).unwrap(),
                "stale"
            );
        }

        #[test]
        fn restore_recovered_files_returns_zero_for_missing_directory() {
            let directory = TempDir::new().unwrap();
            let restored = restore_recovered_files(
                &directory.path().join("missing"),
                &directory.path().join("mount"),
            )
            .unwrap();
            assert_eq!(restored, 0);
        }

        #[test]
        fn copy_recursively_duplicates_files_and_folders_in_place() {
            let directory = TempDir::new().unwrap();
            let source = directory.path().join("source");
            std::fs::create_dir_all(source.join("nested")).unwrap();
            std::fs::write(source.join("a.txt"), b"alpha").unwrap();
            std::fs::write(source.join("nested/b.txt"), b"beta").unwrap();

            copy_recursively(&source, &directory.path().join("destination")).unwrap();
            assert_eq!(
                std::fs::read_to_string(directory.path().join("destination/a.txt")).unwrap(),
                "alpha"
            );
            assert_eq!(
                std::fs::read_to_string(directory.path().join("destination/nested/b.txt")).unwrap(),
                "beta"
            );
            assert!(source.join("a.txt").is_file());
        }

        #[test]
        fn discard_recovered_files_removes_everything_and_counts() {
            let directory = TempDir::new().unwrap();
            let recovered = directory.path().join("recovered");
            std::fs::create_dir_all(recovered.join("folder")).unwrap();
            std::fs::write(recovered.join("a.txt"), b"one").unwrap();
            std::fs::write(recovered.join("folder/b.txt"), b"two").unwrap();

            let discarded = discard_recovered_files(&recovered).unwrap();
            assert_eq!(discarded, 2);
            assert!(!recovered.exists());
        }

        #[test]
        fn discard_recovered_files_returns_zero_when_absent() {
            let directory = TempDir::new().unwrap();
            let discarded = discard_recovered_files(&directory.path().join("missing")).unwrap();
            assert_eq!(discarded, 0);
        }

        #[test]
        fn unmount_candidates_prefer_fusermount3() {
            let candidates = unmount_candidates(Path::new("/data/mount"), false);
            assert_eq!(candidates[0].0, "fusermount3");
            assert_eq!(candidates[0].1, vec!["-u", "/data/mount"]);

            let lazy_candidates = unmount_candidates(Path::new("/data/mount"), true);
            assert_eq!(lazy_candidates[0].1, vec!["-uz", "/data/mount"]);
            assert_eq!(lazy_candidates.last().unwrap().1, vec!["-l", "/data/mount"]);
        }
    }
}
