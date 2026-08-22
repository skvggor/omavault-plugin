use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub const ACCEPTED_PASSPHRASE: &str = "correct horse battery staple";
pub const MASTER_KEY: &str =
    "6f2f38e6-93a3f5ac-1bd0cbb2-6ba0d9a4-ea82e9a7-77c37cd8-a5f0e13d-29f28e1d-59b21c53-8ec30a29-64bfa09e-4a29921e-d74e3ec7";

pub fn fake_gocryptfs(directory: &Path) -> PathBuf {
    let script = directory.join("fake-gocryptfs");
    fs::write(
        &script,
        format!(
            "#!/bin/sh
case \"$1\" in
  -init)
    read passphrase
    read confirmation
    if [ \"$passphrase\" != \"$confirmation\" ] || [ \"$passphrase\" != \"{ACCEPTED_PASSPHRASE}\" ]; then
      echo 'Password dissimilar.' >&2
      exit 1
    fi
    touch \"$2/gocryptfs.conf\"
    echo 'Your master key is:'
    echo ''
    echo '    {MASTER_KEY}'
    ;;
  *)
    if [ \"${{2#-masterkey=}}\" != \"$2\" ]; then
      if [ \"${{2#-masterkey=}}\" = \"{MASTER_KEY}\" ]; then
        exit 0
      fi
      echo 'Could not parse master key: encoding/hex: invalid byte' >&2
      exit 14
    fi
    read passphrase
    if [ \"$passphrase\" != \"{ACCEPTED_PASSPHRASE}\" ]; then
      echo 'Password incorrect.' >&2
      exit 1
    fi
    ;;
esac
"
        ),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    script
}
