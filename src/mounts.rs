pub fn unescape_mount_field(field: &str) -> String {
    field
        .replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\134", "\\")
}

pub fn mount_point_of_line(line: &str) -> Option<&str> {
    let mut parts = line.split(' ');
    parts.next()?;
    parts.next()
}

pub fn is_mounted_text(mounts_content: &str, mount_dir: &str) -> bool {
    mounts_content.lines().any(|line| {
        mount_point_of_line(line)
            .map(|field| unescape_mount_field(field) == mount_dir)
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "proc /proc proc rw 0 0\ntmpfs /home/skvggor/.local/share/omavault/Protected\\040Files fuse.gocryptfs rw 0 0\n/dev/sda1 /home/skvggor/Some\\040Dir ext4 rw 0 0\n";

    #[test]
    fn detects_exact_mount_point() {
        assert!(is_mounted_text(
            SAMPLE,
            "/home/skvggor/.local/share/omavault/Protected Files"
        ));
    }

    #[test]
    fn rejects_prefix_of_longer_path() {
        assert!(!is_mounted_text(
            SAMPLE,
            "/home/skvggor/.local/share/omavault"
        ));
    }

    #[test]
    fn unescapes_octal_sequences_when_comparing() {
        assert!(is_mounted_text(SAMPLE, "/home/skvggor/Some Dir"));
    }

    #[test]
    fn returns_false_for_absent_mount_point() {
        assert!(!is_mounted_text(SAMPLE, "/mnt/other"));
    }

    #[test]
    fn skips_lines_without_mount_field() {
        assert!(!is_mounted_text("garbage-line\n", "/anything"));
    }

    #[test]
    fn unescape_handles_tab_and_backslash() {
        assert_eq!(unescape_mount_field("a\\011b\\134c"), "a\tb\\c");
    }
}
