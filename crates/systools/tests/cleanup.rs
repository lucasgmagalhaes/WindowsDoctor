use std::fs::{self, File};
use std::time::{Duration, SystemTime};

use systools::cleanup::{clean_dirs, measure_dirs};

const DAY: Duration = Duration::from_secs(86_400);

fn write(path: &std::path::Path, len: usize, age: Duration) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![0u8; len]).unwrap();
    let f = File::options().write(true).open(path).unwrap();
    f.set_modified(SystemTime::now() - age).unwrap();
}

#[test]
fn measures_and_cleans_only_old_files_and_keeps_the_root() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Temp");
    write(&root.join("old.tmp"), 100, 3 * DAY);
    write(&root.join("sub/deep/old.log"), 50, 2 * DAY);
    write(&root.join("new/fresh.tmp"), 7, Duration::ZERO);
    let paths = vec![root.clone()];

    let all = measure_dirs(&paths, None, SystemTime::now());
    assert_eq!((all.size, all.files), (157, 3));
    let old = measure_dirs(&paths, Some(DAY), SystemTime::now());
    assert_eq!((old.size, old.files), (150, 2));

    let r = clean_dirs(&paths, Some(DAY), SystemTime::now());
    assert_eq!((r.freed, r.deleted, r.skipped), (150, 2, 0));
    assert!(root.is_dir(), "a pasta raiz é mantida");
    assert!(!root.join("sub").exists(), "subpastas vazias são removidas");
    assert!(root.join("new/fresh.tmp").exists(), "arquivos recentes ficam");
}

#[test]
fn missing_folder_counts_as_unreadable_not_panic() {
    let dir = tempfile::tempdir().unwrap();
    let paths = vec![dir.path().join("nao-existe")];
    let u = measure_dirs(&paths, None, SystemTime::now());
    assert_eq!((u.size, u.files, u.unreadable), (0, 0, 1));
}

#[test]
fn removes_read_only_files() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("ro.bin");
    write(&f, 10, Duration::ZERO);
    let mut perm = fs::metadata(&f).unwrap().permissions();
    perm.set_readonly(true);
    fs::set_permissions(&f, perm).unwrap();
    let r = clean_dirs(&[dir.path().to_path_buf()], None, SystemTime::now());
    assert_eq!(r.deleted, 1);
    assert!(!f.exists());
}

#[cfg(unix)]
#[test]
fn does_not_follow_symlinks() {
    let dir = tempfile::tempdir().unwrap();
    let outside = dir.path().join("importante");
    write(&outside.join("doc.txt"), 10, 5 * DAY);
    let root = dir.path().join("Temp");
    fs::create_dir_all(&root).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
    let r = clean_dirs(std::slice::from_ref(&root), None, SystemTime::now());
    assert_eq!(r.deleted, 0);
    assert!(outside.join("doc.txt").exists());
}
