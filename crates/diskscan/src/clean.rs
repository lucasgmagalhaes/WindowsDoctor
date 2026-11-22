//! Cleaning a folder's *contents* (temporary files, caches).
//!
//! The folder itself is kept. Files modified more recently than `min_age` are
//! kept as well (a program may still be using them), as are symlinks and
//! junctions (deleting through them could reach data outside the folder).
//! Files that cannot be deleted (in use, no permission) are counted in
//! [`CleanReport::failed`] and the cleanup carries on.

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use serde::Serialize;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanReport {
    /// Bytes freed.
    pub freed: u64,
    /// Files deleted.
    pub deleted: u64,
    /// Files that could not be deleted (in use, no permission).
    pub failed: u64,
    /// Files kept because they are too recent.
    pub skipped_recent: u64,
}

impl std::ops::AddAssign for CleanReport {
    fn add_assign(&mut self, o: Self) {
        self.freed += o.freed;
        self.deleted += o.deleted;
        self.failed += o.failed;
        self.skipped_recent += o.skipped_recent;
    }
}

/// Deletes the contents of `dir` (not `dir` itself). See the module docs.
pub fn clean_dir_contents(dir: &Path, min_age: Duration) -> CleanReport {
    let mut report = CleanReport::default();
    let now = SystemTime::now();
    clean_inner(dir, min_age, now, &mut report);
    report
}

fn is_recent(meta: &fs::Metadata, min_age: Duration, now: SystemTime) -> bool {
    if min_age.is_zero() {
        return false;
    }
    match meta.modified() {
        Ok(m) => now.duration_since(m).map(|age| age < min_age).unwrap_or(true),
        Err(_) => true, // no date available: keep it, to be safe
    }
}

fn remove_file(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        // Windows refuses to delete read-only files; clear the flag and retry.
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            let mut perms = fs::metadata(path)?.permissions();
            if !perms.readonly() {
                return Err(e);
            }
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            fs::set_permissions(path, perms)?;
            fs::remove_file(path)
        }
        r => r,
    }
}

/// Returns `true` if `dir` ended up empty.
fn clean_inner(dir: &Path, min_age: Duration, now: SystemTime, r: &mut CleanReport) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        r.failed += 1;
        return false;
    };
    let mut empty = true;
    for entry in entries {
        let Ok(entry) = entry else {
            r.failed += 1;
            empty = false;
            continue;
        };
        let Ok(ft) = entry.file_type() else {
            r.failed += 1;
            empty = false;
            continue;
        };
        let path = entry.path();
        if ft.is_symlink() {
            empty = false;
            continue;
        }
        if ft.is_dir() {
            // Remove the subfolder only if it ended up empty.
            let sub_empty = clean_inner(&path, min_age, now, r);
            if !(sub_empty && fs::remove_dir(&path).is_ok()) {
                empty = false;
            }
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            r.failed += 1;
            empty = false;
            continue;
        };
        if is_recent(&meta, min_age, now) {
            r.skipped_recent += 1;
            empty = false;
            continue;
        }
        match remove_file(&path) {
            Ok(()) => {
                r.freed += meta.len();
                r.deleted += 1;
            }
            Err(_) => {
                r.failed += 1;
                empty = false;
            }
        }
    }
    empty
}

/// Total size and file count under `path` (a folder or a single file).
/// Returns `None` if it cannot be read.
pub fn measure(path: &Path) -> Option<(u64, u64)> {
    let meta = fs::symlink_metadata(path).ok()?;
    if meta.is_file() {
        return Some((meta.len(), 1));
    }
    if !meta.is_dir() {
        return None;
    }
    // Every file goes into the "others" group: no nodes per file, little memory.
    let opts = crate::ScanOptions {
        min_file_size: u64::MAX,
        top_files: 0,
    };
    let tree = crate::scan(path, &opts, &crate::Progress::new()).ok()?;
    let s = tree.stats();
    Some((s.total_size, s.file_count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    fn write(path: &Path, size: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![1u8; size]).unwrap();
    }

    fn age(path: &Path, secs: u64) {
        let t = SystemTime::now() - Duration::from_secs(secs);
        File::options().write(true).open(path).unwrap().set_modified(t).unwrap();
    }

    #[test]
    fn deletes_contents_and_keeps_the_folder() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("Temp");
        write(&root.join("a.tmp"), 100);
        write(&root.join("sub/b.tmp"), 50);
        write(&root.join("sub/deep/c.tmp"), 25);
        let r = clean_dir_contents(&root, Duration::ZERO);
        assert_eq!(r, CleanReport { freed: 175, deleted: 3, failed: 0, skipped_recent: 0 });
        assert!(root.is_dir());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0, "empty subfolders are removed");
    }

    #[test]
    fn keeps_recent_files_and_their_folders() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        write(&root.join("old.tmp"), 10);
        age(&root.join("old.tmp"), 3 * 86_400);
        write(&root.join("sub/new.tmp"), 20);
        write(&root.join("sub/old.tmp"), 30);
        age(&root.join("sub/old.tmp"), 3 * 86_400);
        let r = clean_dir_contents(root, Duration::from_secs(86_400));
        assert_eq!((r.freed, r.deleted, r.skipped_recent), (40, 2, 1));
        assert!(root.join("sub/new.tmp").exists());
        assert!(!root.join("old.tmp").exists());
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlinks() {
        let d = tempfile::tempdir().unwrap();
        let precious = d.path().join("precious");
        write(&precious.join("doc.txt"), 10);
        let root = d.path().join("cache");
        write(&root.join("x"), 5);
        std::os::unix::fs::symlink(&precious, root.join("link")).unwrap();
        let r = clean_dir_contents(&root, Duration::ZERO);
        assert_eq!(r.deleted, 1);
        assert!(precious.join("doc.txt").exists(), "the symlink target must not be touched");
    }

    #[test]
    fn missing_folder_is_reported_as_failure() {
        let d = tempfile::tempdir().unwrap();
        let r = clean_dir_contents(&d.path().join("missing"), Duration::ZERO);
        assert_eq!(r.failed, 1);
        assert_eq!(r.deleted, 0);
    }

    #[cfg(unix)]
    #[test]
    fn read_only_files_are_deleted() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("ro.tmp");
        write(&f, 7);
        let mut p = fs::metadata(&f).unwrap().permissions();
        p.set_readonly(true);
        fs::set_permissions(&f, p).unwrap();
        assert_eq!(clean_dir_contents(d.path(), Duration::ZERO).deleted, 1);
    }

    #[test]
    fn measure_file_and_folder() {
        let d = tempfile::tempdir().unwrap();
        write(&d.path().join("a/x"), 100);
        write(&d.path().join("a/b/y"), 23);
        assert_eq!(measure(&d.path().join("a")), Some((123, 2)));
        assert_eq!(measure(&d.path().join("a/x")), Some((100, 1)));
        assert_eq!(measure(&d.path().join("nope")), None);
    }
}
