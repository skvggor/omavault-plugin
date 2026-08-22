use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub folder: String,
    pub modified_ts: i64,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanSummary {
    pub total_bytes: u64,
    pub file_count: usize,
    pub files: Vec<FileEntry>,
}

pub fn scan_recent(root: &Path, limit: usize) -> ScanSummary {
    let mut entries: Vec<FileEntry> = Vec::new();
    let mut total_bytes = 0;
    let mut file_count = 0;
    collect(root, root, &mut entries, &mut total_bytes, &mut file_count);
    entries.sort_by(|left, right| {
        right
            .modified_ts
            .cmp(&left.modified_ts)
            .then_with(|| left.name.cmp(&right.name))
    });
    entries.truncate(limit.max(1));
    ScanSummary {
        total_bytes,
        file_count,
        files: entries,
    }
}

fn collect(
    root: &Path,
    directory: &Path,
    entries: &mut Vec<FileEntry>,
    total_bytes: &mut u64,
    file_count: &mut usize,
) {
    let Ok(children) = std::fs::read_dir(directory) else {
        return;
    };
    for child in children.flatten() {
        let path = child.path();
        if path.is_symlink() {
            continue;
        }
        if path.is_dir() {
            if !is_trash_directory(&child.file_name().to_string_lossy()) {
                collect(root, &path, entries, total_bytes, file_count);
            }
            continue;
        }
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let folder = relative
            .parent()
            .map(|parent| parent.to_string_lossy().to_string())
            .filter(|parent| !parent.is_empty())
            .unwrap_or_else(|| "/".to_string());
        *total_bytes += metadata.len();
        *file_count += 1;
        entries.push(FileEntry {
            name: relative
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default(),
            path: path.to_string_lossy().to_string(),
            folder,
            modified_ts: metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs() as i64)
                .unwrap_or(0),
            size_bytes: metadata.len(),
        });
    }
}

fn is_trash_directory(name: &str) -> bool {
    name == ".Trash" || name.starts_with(".Trash-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn write_file(root: &Path, relative: &str, bytes: &[u8], age_seconds: u64) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        let modified = std::time::SystemTime::now() - std::time::Duration::from_secs(age_seconds);
        fs::File::options()
            .append(true)
            .open(&path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
    }

    #[test]
    fn scans_nested_files_sorted_by_modification_time() {
        let directory = TempDir::new().unwrap();
        let root = directory.path();
        write_file(root, "z-old.txt", b"1", 900);
        write_file(root, "nested/a-new.md", b"1234", 100);
        write_file(root, "b-newest.txt", b"12", 10);

        let summary = scan_recent(root, 10);
        assert_eq!(summary.file_count, 3);
        assert_eq!(summary.total_bytes, 7);
        let names: Vec<&str> = summary
            .files
            .iter()
            .map(|file| file.name.as_str())
            .collect();
        assert_eq!(names, vec!["b-newest.txt", "a-new.md", "z-old.txt"]);
        assert_eq!(summary.files[1].folder, "nested");
    }

    #[test]
    fn respects_limit() {
        let directory = TempDir::new().unwrap();
        let root = directory.path();
        write_file(root, "one.txt", b"1", 30);
        write_file(root, "two.txt", b"1", 20);
        write_file(root, "three.txt", b"1", 10);
        let summary = scan_recent(root, 2);
        assert_eq!(summary.files.len(), 2);
        assert_eq!(summary.file_count, 3);
    }

    #[test]
    fn skips_symlinks_and_unreadable_directories() {
        let directory = TempDir::new().unwrap();
        let root = directory.path();
        write_file(root, "real.txt", b"1", 5);
        std::os::unix::fs::symlink(root.join("real.txt"), root.join("link.txt")).unwrap();
        let blocked = root.join("blocked");
        fs::create_dir(&blocked).unwrap();
        write_file(&blocked, "hidden.txt", b"1", 5);
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).unwrap();

        let summary = scan_recent(root, 10);
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(summary.file_count, 1);
        assert_eq!(summary.files[0].name, "real.txt");
    }

    #[test]
    fn excludes_freedesktop_trash_directories() {
        let directory = TempDir::new().unwrap();
        let root = directory.path();
        write_file(root, "kept.txt", b"1", 5);
        write_file(root, ".Trash-1000/files/deleted.txt", b"22", 4);
        write_file(
            root,
            ".Trash-1000/info/deleted.txt.trashinfo",
            b"[Trash Info]",
            3,
        );
        write_file(root, ".Trash/files/legacy.txt", b"333", 2);

        let summary = scan_recent(root, 10);
        assert_eq!(summary.file_count, 1);
        assert_eq!(summary.total_bytes, 1);
        assert_eq!(summary.files[0].name, "kept.txt");
    }

    #[test]
    fn root_files_report_slash_folder() {
        let directory = TempDir::new().unwrap();
        write_file(directory.path(), "top.txt", b"1", 5);
        let summary = scan_recent(directory.path(), 10);
        assert_eq!(summary.files[0].folder, "/");
    }

    #[test]
    fn empty_root_returns_empty_summary() {
        let directory = TempDir::new().unwrap();
        let summary = scan_recent(directory.path(), 10);
        assert_eq!(summary.file_count, 0);
        assert_eq!(summary.total_bytes, 0);
        assert!(summary.files.is_empty());
    }

    #[test]
    fn missing_root_returns_empty_summary() {
        let summary = scan_recent(Path::new("/nonexistent/omavault-root"), 10);
        assert_eq!(summary.file_count, 0);
        assert!(summary.files.is_empty());
    }
}
