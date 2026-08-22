use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holder {
    pub process: String,
    pub open_paths: Vec<String>,
}

pub const MAX_PATHS_PER_HOLDER: usize = 10;
const IGNORED_PROCESSES: [&str; 2] = ["gocryptfs", "omavault-helper"];

pub fn scan_holders(mount_dir: &Path) -> Vec<Holder> {
    scan_holders_from(Path::new("/proc"), mount_dir)
}

pub fn scan_holders_from(proc_root: &Path, mount_dir: &Path) -> Vec<Holder> {
    let mut holders: Vec<Holder> = Vec::new();
    let Ok(entries) = std::fs::read_dir(proc_root) else {
        return holders;
    };
    let own_pid = std::process::id().to_string();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let pid = name.to_string_lossy().to_string();
        if !name.to_string_lossy().chars().all(|character| character.is_ascii_digit()) {
            continue;
        }
        if pid == own_pid {
            continue;
        }
        let process = std::fs::read_to_string(entry.path().join("comm"))
            .map(|value| value.trim().to_string())
            .unwrap_or_default();
        if process.is_empty() || IGNORED_PROCESSES.contains(&process.as_str()) {
            continue;
        }
        let mut open_paths: Vec<String> = Vec::new();
        collect_link(&entry.path().join("cwd"), mount_dir, &mut open_paths);
        if let Ok(fds) = std::fs::read_dir(entry.path().join("fd")) {
            for fd in fds.flatten() {
                collect_link(&fd.path(), mount_dir, &mut open_paths);
            }
        }
        if open_paths.is_empty() {
            continue;
        }
        match holders.iter_mut().find(|holder| holder.process == process) {
            Some(holder) => holder.open_paths.extend(open_paths),
            None => holders.push(Holder { process, open_paths }),
        }
    }
    for holder in &mut holders {
        holder.open_paths.sort();
        holder.open_paths.dedup();
        holder.open_paths.truncate(MAX_PATHS_PER_HOLDER);
    }
    holders.sort_by(|left, right| left.process.cmp(&right.process));
    holders
}

fn collect_link(link: &Path, mount_dir: &Path, open_paths: &mut Vec<String>) {
    if let Ok(resolved) = std::fs::read_link(link) {
        if resolved.starts_with(mount_dir) {
            open_paths.push(resolved.to_string_lossy().to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn fake_holder(root: &Path, pid: &str, comm: &str, links: &[&std::path::Path]) {
        let directory = root.join(pid);
        fs::create_dir_all(directory.join("fd")).unwrap();
        fs::write(directory.join("comm"), comm).unwrap();
        for (index, target) in links.iter().enumerate() {
            std::os::unix::fs::symlink(target, directory.join(format!("fd/{}", index))).unwrap();
        }
    }

    fn vault(directory: &TempDir) -> std::path::PathBuf {
        let vault = directory.path().join("Protected Files");
        fs::create_dir_all(vault.join("docs")).unwrap();
        vault
    }

    #[test]
    fn groups_open_paths_by_process_name() {
        let directory = TempDir::new().unwrap();
        let mount = vault(&directory);
        fake_holder(directory.path(), "100", "nvim\n", &[&mount.join("docs/a.md"), &mount.join("docs/b.md")]);
        fake_holder(directory.path(), "200", "nvim\n", &[&mount.join("docs/c.md")]);
        fake_holder(directory.path(), "300", "nautilus\n", &[&mount]);

        let holders = scan_holders_from(directory.path(), &mount);
        assert_eq!(holders.len(), 2);
        assert_eq!(holders[0].process, "nautilus");
        assert_eq!(holders[1].process, "nvim");
        assert_eq!(holders[1].open_paths.len(), 3);
    }

    #[test]
    fn ignores_paths_outside_the_mount() {
        let directory = TempDir::new().unwrap();
        let mount = vault(&directory);
        fake_holder(directory.path(), "100", "sleep\n", &[&std::path::PathBuf::from("/tmp")]);

        assert!(scan_holders_from(directory.path(), &mount).is_empty());
    }

    #[test]
    fn skips_vault_and_helper_processes() {
        let directory = TempDir::new().unwrap();
        let mount = vault(&directory);
        fake_holder(directory.path(), "100", "gocryptfs\n", &[&mount]);
        fake_holder(directory.path(), "200", "omavault-helper\n", &[&mount]);

        assert!(scan_holders_from(directory.path(), &mount).is_empty());
    }

    #[test]
    fn caps_reported_paths_per_process() {
        let directory = TempDir::new().unwrap();
        let mount = vault(&directory);
        let links: Vec<std::path::PathBuf> = (0..MAX_PATHS_PER_HOLDER + 5)
            .map(|index| mount.join(format!("file-{}.txt", index)))
            .collect();
        let references: Vec<&std::path::Path> = links.iter().map(std::convert::AsRef::as_ref).collect();
        fake_holder(directory.path(), "100", "nvim\n", &references);

        let holders = scan_holders_from(directory.path(), &mount);
        assert_eq!(holders[0].open_paths.len(), MAX_PATHS_PER_HOLDER);
    }

    #[test]
    fn missing_proc_root_returns_empty() {
        assert!(scan_holders_from(Path::new("/nonexistent-holders-proc"), Path::new("/vault")).is_empty());
    }

    #[test]
    fn unreadable_fd_directory_is_tolerated() {
        let directory = TempDir::new().unwrap();
        let mount = vault(&directory);
        let blocked = directory.path().join("100");
        fs::create_dir_all(blocked.join("fd")).unwrap();
        fs::write(blocked.join("comm"), "nvim\n").unwrap();
        std::os::unix::fs::symlink(&mount, blocked.join("cwd")).unwrap();
        fs::set_permissions(blocked.join("fd"), fs::Permissions::from_mode(0o000)).unwrap();

        let holders = scan_holders_from(directory.path(), &mount);
        fs::set_permissions(blocked.join("fd"), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(holders.len(), 1);
        assert_eq!(holders[0].process, "nvim");
    }
}
