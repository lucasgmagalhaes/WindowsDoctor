//! Edge-case tests for the scanner: unusual names, deep trees, links,
//! cancellation and tree consistency after changes.

use std::fs;
use std::path::Path;

use diskscan::{scan, NodeKind, Progress, ScanOptions, Tree};

fn write(path: &Path, size: usize) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![0u8; size]).unwrap();
}

fn opts(min: u64, top: usize) -> ScanOptions {
    ScanOptions {
        min_file_size: min,
        top_files: top,
    }
}

/// Sums the sizes of the direct children and compares them with the parent's size, recursively.
fn assert_consistent(t: &Tree, id: u32) {
    let v = t.view(id, 1, usize::MAX, 0).unwrap();
    if v.kind == NodeKind::Dir {
        let sum: u64 = v.children.iter().map(|c| c.size).sum();
        assert_eq!(sum, v.size, "size of {:?} differs from its children's sum", t.path_of(id));
        let files: u64 = v.children.iter().map(|c| c.file_count).sum();
        assert_eq!(files, v.file_count, "file count of {:?}", t.path_of(id));
        for c in &v.children {
            assert!(c.kind != NodeKind::Rest);
            assert_consistent(t, c.id.unwrap());
        }
    }
}

#[test]
fn root_that_is_a_file_is_rejected() {
    let d = tempfile::tempdir().unwrap();
    write(&d.path().join("f"), 1);
    assert!(scan(&d.path().join("f"), &opts(0, 10), &Progress::new()).is_err());
    assert!(scan(&d.path().join("does-not-exist"), &opts(0, 10), &Progress::new()).is_err());
}

#[test]
fn empty_root() {
    let d = tempfile::tempdir().unwrap();
    let t = scan(d.path(), &opts(0, 10), &Progress::new()).unwrap();
    let s = t.stats();
    assert_eq!((s.total_size, s.file_count, s.dir_count), (0, 0, 0));
    assert!(t.view(0, 3, 10, 0).unwrap().children.is_empty());
    assert!(t.top_files(10).is_empty());
}

#[test]
fn unicode_and_special_names() {
    let d = tempfile::tempdir().unwrap();
    let mut names = vec!["ação çã", "日本語", "emoji 🎉", "com espaço", "a,b;c", ".oculto"];
    // Windows does not allow names ending in a space or a dot.
    if cfg!(unix) {
        names.extend(["espaço no fim ", "ponto no fim."]);
    }
    for (i, n) in names.iter().enumerate() {
        write(&d.path().join(n).join("f.bin"), 100 * (i + 1));
    }
    let t = scan(d.path(), &opts(0, 10), &Progress::new()).unwrap();
    for n in names {
        let p = d.path().join(n);
        let id = t.find(&p).unwrap_or_else(|| panic!("did not find {n:?}"));
        assert_eq!(t.path_of(id).unwrap(), p);
    }
    assert_consistent(&t, 0);
}

#[test]
fn deep_tree_does_not_overflow_the_stack() {
    let d = tempfile::tempdir().unwrap();
    let mut p = d.path().to_path_buf();
    // 1000 levels with short names (the OS path limit is ~4096 on Linux).
    for _ in 0..1000 {
        p.push("d");
    }
    fs::create_dir_all(&p).unwrap();
    fs::write(p.join("f"), b"xyz").unwrap();
    let t = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024) // the default size of a normal thread
        .spawn({
            let root = d.path().to_path_buf();
            move || scan(&root, &opts(0, 10), &Progress::new()).unwrap()
        })
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(t.stats().total_size, 3);
    assert_eq!(t.stats().dir_count, 1000);
}

#[cfg(unix)]
#[test]
fn symlinks_are_not_followed() {
    let d = tempfile::tempdir().unwrap();
    write(&d.path().join("real/big.bin"), 5000);
    // A loop and a duplicate pointer: neither should be counted.
    std::os::unix::fs::symlink(d.path(), d.path().join("real/loop")).unwrap();
    std::os::unix::fs::symlink(d.path().join("real"), d.path().join("dup")).unwrap();
    std::os::unix::fs::symlink(d.path().join("real/big.bin"), d.path().join("link.bin")).unwrap();
    let t = scan(d.path(), &opts(0, 10), &Progress::new()).unwrap();
    assert_eq!(t.stats().total_size, 5000);
    assert_eq!(t.stats().file_count, 1);
}

#[test]
fn cancel_before_start_returns_empty_marked_tree() {
    let d = tempfile::tempdir().unwrap();
    write(&d.path().join("a/f"), 10);
    let p = Progress::new();
    p.cancel();
    let t = scan(d.path(), &opts(0, 10), &p).unwrap();
    assert!(t.stats().cancelled);
    assert_eq!(t.stats().total_size, 0);
}

#[test]
fn others_node_and_root_cannot_be_removed() {
    let d = tempfile::tempdir().unwrap();
    write(&d.path().join("small"), 10);
    let mut t = scan(d.path(), &opts(1000, 10), &Progress::new()).unwrap();
    let v = t.view(0, 1, 10, 0).unwrap();
    assert_eq!(v.children[0].kind, NodeKind::Others);
    assert_eq!(t.remove(v.children[0].id.unwrap()), None);
    assert_eq!(t.remove(0), None);
}

#[test]
fn many_files_and_consistency_after_removals() {
    let d = tempfile::tempdir().unwrap();
    for i in 0..30 {
        for j in 0..20 {
            write(&d.path().join(format!("d{i}/s{j}/f.bin")), (i * 37 + j * 11) % 500 + 1);
        }
    }
    let mut t = scan(d.path(), &opts(200, 50), &Progress::new()).unwrap();
    assert_eq!(t.stats().file_count, 600);
    assert_consistent(&t, 0);
    for i in (0..30).step_by(3) {
        let id = t.find(&d.path().join(format!("d{i}"))).unwrap();
        t.remove(id).unwrap();
        assert_consistent(&t, 0);
    }
    assert_eq!(t.stats().file_count, 400);
    assert_eq!(t.stats().dir_count, 20 + 20 * 20);
}

#[test]
fn removing_a_file_does_not_invalidate_other_ids() {
    let d = tempfile::tempdir().unwrap();
    write(&d.path().join("a/x"), 300);
    write(&d.path().join("a/y"), 200);
    write(&d.path().join("b"), 100);
    let mut t = scan(d.path(), &opts(0, 10), &Progress::new()).unwrap();
    let y = t.find(&d.path().join("a/y")).unwrap();
    let b = t.find(&d.path().join("b")).unwrap();
    t.remove(t.find(&d.path().join("a/x")).unwrap()).unwrap();
    assert_eq!(t.path_of(y).unwrap(), d.path().join("a/y"));
    assert_eq!(t.path_of(b).unwrap(), d.path().join("b"));
    // The removed one no longer shows up in queries.
    assert!(t.find(&d.path().join("a/x")).is_none());
}

/// A rescan of a subfolder must not shrink the list of largest files when the
/// original scan found fewer files than the limit.
#[test]
fn rescan_keeps_the_top_files_limit() {
    let d = tempfile::tempdir().unwrap();
    write(&d.path().join("a/one"), 100);
    let mut t = scan(d.path(), &opts(0, 500), &Progress::new()).unwrap();
    write(&d.path().join("a/two"), 200);
    write(&d.path().join("a/three"), 300);
    let a = t.find(&d.path().join("a")).unwrap();
    let sub = scan(&d.path().join("a"), &opts(0, 500), &Progress::new()).unwrap();
    t.replace_subtree(a, sub).unwrap();
    assert_eq!(t.top_files(500).len(), 3);
}

/// Files below the grouping threshold still show up in the largest files
/// list (small partitions) and must be removable from the tree.
#[test]
fn small_file_from_top_list_can_be_removed() {
    let d = tempfile::tempdir().unwrap();
    write(&d.path().join("a/small1"), 30);
    write(&d.path().join("a/small2"), 20);
    let mut t = scan(d.path(), &opts(1000, 10), &Progress::new()).unwrap();
    let path = d.path().join("a/small1");
    assert!(t.top_files(10).iter().any(|f| Path::new(&f.path) == path));
    assert!(t.find(&path).is_none(), "small file is grouped, has no node of its own");
    assert_eq!(t.remove_path(&path), Some(30));
    assert_eq!(t.stats().total_size, 20);
    assert_eq!(t.stats().file_count, 1);
    assert!(t.top_files(10).iter().all(|f| Path::new(&f.path) != path));
    assert_consistent(&t, 0);
    // The last small file takes the "others" node with it.
    assert_eq!(t.remove_path(&d.path().join("a/small2")), Some(20));
    let a = t.find(&d.path().join("a")).unwrap();
    assert!(t.view(a, 1, 10, 0).unwrap().children.is_empty());
}
