use std::fs;

use diskscan::{scan, Progress, ScanOptions};

#[test]
fn sums_space_by_extension_including_small_files() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    fs::create_dir_all(r.join("a/b")).unwrap();
    fs::write(r.join("a/video.MP4"), vec![0u8; 3000]).unwrap();
    fs::write(r.join("a/b/clip.mp4"), vec![0u8; 1000]).unwrap();
    fs::write(r.join("a/nota.txt"), vec![0u8; 10]).unwrap();
    fs::write(r.join("Makefile"), vec![0u8; 5]).unwrap();
    fs::write(r.join(".gitignore"), vec![0u8; 7]).unwrap();
    let opts = ScanOptions {
        min_file_size: 2000,
        ..ScanOptions::default()
    };
    let t = scan(r, &opts, &Progress::new()).unwrap();
    let ext = t.extensions();
    assert_eq!(ext[0].ext, "mp4");
    assert_eq!((ext[0].size, ext[0].count), (4000, 2));
    let none = ext.iter().find(|e| e.ext.is_empty()).unwrap();
    assert_eq!((none.size, none.count), (12, 2), "sem extensão e arquivos ocultos");
    assert!(ext.iter().any(|e| e.ext == "txt" && e.size == 10));
}
