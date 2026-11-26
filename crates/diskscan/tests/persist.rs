use std::fs;
use std::io::Cursor;

use diskscan::{scan, Progress, ScanOptions, Tree};

#[test]
fn save_and_load_round_trip_including_removals() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    fs::create_dir_all(r.join("fotos/2024")).unwrap();
    fs::create_dir_all(r.join("jogos")).unwrap();
    fs::write(r.join("fotos/2024/a.jpg"), vec![0u8; 5000]).unwrap();
    fs::write(r.join("jogos/grande.pak"), vec![0u8; 9000]).unwrap();
    fs::write(r.join("jogos/ção ü.txt"), vec![0u8; 3]).unwrap();
    let opts = ScanOptions { min_file_size: 1000, ..ScanOptions::default() };
    let mut t = scan(r, &opts, &Progress::new()).unwrap();
    let pak = t.find(&r.join("jogos/grande.pak")).unwrap();
    t.remove(pak).unwrap();

    let mut buf = Vec::new();
    t.save(&mut buf).unwrap();
    let back = Tree::load(Cursor::new(&buf)).unwrap();

    let (a, b) = (t.stats(), back.stats());
    assert_eq!((a.total_size, a.file_count, a.dir_count), (b.total_size, b.file_count, b.dir_count));
    assert_eq!(a.root, b.root);
    assert_eq!(back.find(&r.join("jogos/grande.pak")), None, "a exclusão continua valendo");
    let id = back.find(&r.join("fotos/2024")).unwrap();
    assert_eq!(back.path_of(id).unwrap(), r.join("fotos/2024"));
    assert_eq!(back.extensions(), t.extensions());
    let va = t.view(t.root_id(), 3, 100, 0).unwrap();
    let vb = back.view(back.root_id(), 3, 100, 0).unwrap();
    assert_eq!(format!("{va:?}"), format!("{vb:?}"));
}

#[test]
fn rejects_other_files_and_truncation() {
    assert!(Tree::load(Cursor::new(b"PK\x03\x04 isto e um zip".to_vec())).is_err());
    let d = tempfile::tempdir().unwrap();
    fs::write(d.path().join("x"), b"1").unwrap();
    let t = scan(d.path(), &ScanOptions::default(), &Progress::new()).unwrap();
    let mut buf = Vec::new();
    t.save(&mut buf).unwrap();
    buf.truncate(buf.len() - 3);
    assert!(Tree::load(Cursor::new(buf)).is_err());
}
