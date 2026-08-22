use std::path::PathBuf;

pub const ROOT_ENV_VAR: &str = "OMAVAULT_ROOT";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub root: PathBuf,
}

impl Layout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn cipher_dir(&self) -> PathBuf {
        self.root.join("vault")
    }

    pub fn mount_dir(&self) -> PathBuf {
        self.root.join("Protected Files")
    }

    pub fn recovered_dir(&self) -> PathBuf {
        self.root.join("recovered")
    }

    pub fn is_initialized(&self) -> bool {
        self.cipher_dir().join("gocryptfs.conf").is_file()
    }
}

pub fn resolve_root(env_root: Option<&str>, home: Option<&str>) -> Result<PathBuf, String> {
    if let Some(root) = env_root.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(root));
    }
    match home.map(str::trim).filter(|value| !value.is_empty()) {
        Some(home) => Ok(PathBuf::from(home).join(".local/share/omavault")),
        None => Err("neither OMAVAULT_ROOT nor HOME is set".to_string()),
    }
}

pub fn detect_root() -> Result<PathBuf, String> {
    let env_root = std::env::var(ROOT_ENV_VAR).ok();
    let home = std::env::var("HOME").ok();
    resolve_root(env_root.as_deref(), home.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_derives_cipher_and_mount_dirs() {
        let layout = Layout::new("/data/protected");
        assert_eq!(layout.cipher_dir(), PathBuf::from("/data/protected/vault"));
        assert_eq!(layout.mount_dir(), PathBuf::from("/data/protected/Protected Files"));
        assert_eq!(layout.recovered_dir(), PathBuf::from("/data/protected/recovered"));
    }

    #[test]
    fn resolve_root_prefers_env_var_over_home() {
        let root = resolve_root(Some("/custom/root"), Some("/home/user")).unwrap();
        assert_eq!(root, PathBuf::from("/custom/root"));
    }

    #[test]
    fn resolve_root_ignores_blank_env_var() {
        let root = resolve_root(Some("   "), Some("/home/user")).unwrap();
        assert_eq!(root, PathBuf::from("/home/user/.local/share/omavault"));
    }

    #[test]
    fn resolve_root_falls_back_to_xdg_share_under_home() {
        let root = resolve_root(None, Some("/home/user")).unwrap();
        assert_eq!(root, PathBuf::from("/home/user/.local/share/omavault"));
    }

    #[test]
    fn resolve_root_fails_without_env_var_and_home() {
        assert!(resolve_root(None, None).is_err());
    }

    #[test]
    fn resolve_root_fails_with_blank_home() {
        assert!(resolve_root(None, Some("  ")).is_err());
    }
}
