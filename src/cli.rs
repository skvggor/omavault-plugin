pub const USAGE: &str = "usage: omavault-helper <command> [options]

Commands:
  status   Print vault status as JSON
  init     Create the vault (passphrase on stdin)
  unlock   Mount the decrypted vault (secret on stdin)
  set-passphrase  Reset the passphrase with the recovery key
                  (recovery key and new passphrase on stdin)
  lock     Unmount the decrypted vault
  restore  Move files from 'recovered' back into the unlocked vault
  discard  Permanently delete files in 'recovered'

Options:
  --limit N          Recent files to list in status (default 10, max 100)
  --lazy             Lazy unmount when locking
  --recovery-key     Unlock with the recovery key instead of the passphrase
  -h, --help         Show this help
  -V, --version      Print the helper version as JSON

The vault root is $OMAVAULT_ROOT or ~/.local/share/omavault.";

pub const DEFAULT_LIMIT: usize = 10;
pub const MAX_LIMIT: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Status { limit: usize },
    Init,
    Unlock { recovery_key: bool },
    SetPassphrase,
    Lock { lazy: bool },
    Restore,
    Discard,
    Help,
    Version,
}

pub fn parse(arguments: &[String]) -> Result<Command, String> {
    let mut name: Option<&str> = None;
    let mut limit = DEFAULT_LIMIT;
    let mut lazy = false;
    let mut recovery_key = false;
    let mut index = 0;
    while index < arguments.len() {
        let argument = arguments[index].as_str();
        match argument {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "--limit" => {
                index += 1;
                let value = arguments
                    .get(index)
                    .ok_or_else(|| "--limit requires a value".to_string())?;
                limit = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --limit value: {}", value))?;
            }
            "--lazy" => lazy = true,
            "--recovery-key" => recovery_key = true,
            _ if name.is_none() => name = Some(argument),
            _ => return Err(format!("unexpected argument: {}", argument)),
        }
        index += 1;
    }
    let limit = limit.clamp(1, MAX_LIMIT);
    match name {
        Some("status") => Ok(Command::Status { limit }),
        Some("init") => Ok(Command::Init),
        Some("unlock") => Ok(Command::Unlock { recovery_key }),
        Some("set-passphrase") => Ok(Command::SetPassphrase),
        Some("lock") => Ok(Command::Lock { lazy }),
        Some("restore") => Ok(Command::Restore),
        Some("discard") => Ok(Command::Discard),
        Some(other) => Err(format!("unknown command: {}", other)),
        None => Err(format!("missing command\n{}", USAGE)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::ROOT_ENV_VAR;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn parses_status_with_default_limit() {
        assert_eq!(
            parse(&args(&["status"])).unwrap(),
            Command::Status { limit: 10 }
        );
    }

    #[test]
    fn parses_status_with_custom_limit() {
        assert_eq!(
            parse(&args(&["status", "--limit", "5"])).unwrap(),
            Command::Status { limit: 5 }
        );
    }

    #[test]
    fn clamps_limit_to_maximum() {
        assert_eq!(
            parse(&args(&["status", "--limit", "500"])).unwrap(),
            Command::Status { limit: MAX_LIMIT }
        );
    }

    #[test]
    fn clamps_limit_to_minimum() {
        assert_eq!(
            parse(&args(&["status", "--limit", "0"])).unwrap(),
            Command::Status { limit: 1 }
        );
    }

    #[test]
    fn rejects_invalid_limit() {
        assert!(parse(&args(&["status", "--limit", "abc"])).is_err());
    }

    #[test]
    fn rejects_limit_without_value() {
        assert!(parse(&args(&["status", "--limit"])).is_err());
    }

    #[test]
    fn parses_init_unlock_and_lock() {
        assert_eq!(parse(&args(&["init"])).unwrap(), Command::Init);
        assert_eq!(
            parse(&args(&["unlock"])).unwrap(),
            Command::Unlock {
                recovery_key: false
            }
        );
        assert_eq!(
            parse(&args(&["unlock", "--recovery-key"])).unwrap(),
            Command::Unlock { recovery_key: true }
        );
        assert_eq!(
            parse(&args(&["lock"])).unwrap(),
            Command::Lock { lazy: false }
        );
        assert_eq!(
            parse(&args(&["lock", "--lazy"])).unwrap(),
            Command::Lock { lazy: true }
        );
    }

    #[test]
    fn parses_set_passphrase() {
        assert_eq!(
            parse(&args(&["set-passphrase"])).unwrap(),
            Command::SetPassphrase
        );
    }

    #[test]
    fn parses_restore_and_discard() {
        assert_eq!(parse(&args(&["restore"])).unwrap(), Command::Restore);
        assert_eq!(parse(&args(&["discard"])).unwrap(), Command::Discard);
    }

    #[test]
    fn rejects_root_option() {
        assert!(parse(&args(&["status", "--root", "/tmp/other"])).is_err());
    }

    #[test]
    fn rejects_root_without_value() {
        assert!(parse(&args(&["status", "--root"])).is_err());
    }

    #[test]
    fn parses_help_flags() {
        assert_eq!(parse(&args(&["--help"])).unwrap(), Command::Help);
        assert_eq!(parse(&args(&["-h"])).unwrap(), Command::Help);
    }

    #[test]
    fn rejects_unknown_command() {
        assert!(parse(&args(&["explode"])).is_err());
    }

    #[test]
    fn rejects_missing_command() {
        assert!(parse(&args(&[])).is_err());
    }

    #[test]
    fn rejects_unexpected_extra_positional() {
        assert!(parse(&args(&["status", "extra"])).is_err());
    }

    #[test]
    fn usage_mentions_env_var() {
        assert!(USAGE.contains(ROOT_ENV_VAR));
    }
}
