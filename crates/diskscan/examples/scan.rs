//! Varredura pela linha de comando, útil para testar desempenho:
//! `cargo run --release -p diskscan --example scan -- C:\`
use std::path::PathBuf;

fn human(n: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let (mut v, mut i) = (n as f64, 0);
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", units[i])
}

fn main() {
    let root = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    let progress = diskscan::Progress::new();
    let tree = diskscan::scan(&root, &diskscan::ScanOptions::default(), &progress).expect("falha na varredura");
    let s = tree.stats();
    println!(
        "{}: {} em {} arquivos / {} pastas ({} erros) em {} ms\n",
        s.root, human(s.total_size), s.file_count, s.dir_count, s.errors, s.elapsed_ms
    );
    let view = tree.view(tree.root_id(), 1, 15, 0).unwrap();
    for c in view.children {
        println!("{:>10}  {:?}  {}", human(c.size), c.kind, c.name);
    }
    println!("\nMaiores arquivos:");
    for f in tree.top_files(10) {
        println!("{:>10}  {}", human(f.size), f.path);
    }
}
