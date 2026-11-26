//! Arquivos duplicados: mesmo tamanho, mesmo hash e conteúdo idêntico byte a
//! byte. Pastas de sistema e de programas ficam de fora (apagar cópias ali
//! quebra programas) e arquivos só na nuvem não são lidos.

use std::collections::HashMap;
use std::fs::{self, File};
use std::hash::{DefaultHasher, Hasher};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use serde::Serialize;

/// Pastas puladas pelo nome (qualquer nível).
const SKIP_DIRS: [&str; 10] = [
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "appdata",
    "$recycle.bin",
    "system volume information",
    "node_modules",
    ".git",
    "windowsapps",
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub size: u64,
    /// Espaço liberado mantendo uma cópia.
    pub wasted: u64,
    pub paths: Vec<String>,
}

#[derive(Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub hashed: AtomicU64,
    pub cancel: AtomicBool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub files: u64,
    pub hashed: u64,
}

impl Progress {
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            files: self.files.load(Ordering::Relaxed),
            hashed: self.hashed.load(Ordering::Relaxed),
        }
    }
}

fn skip_dir(name: &str) -> bool {
    let n = name.to_lowercase();
    SKIP_DIRS.contains(&n.as_str())
}

/// Arquivo só na nuvem (OneDrive etc.): ler forçaria o download.
fn cloud_only(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const OFFLINE: u32 = 0x1000;
        const RECALL_ON_OPEN: u32 = 0x40000;
        const RECALL_ON_DATA_ACCESS: u32 = 0x400000;
        meta.file_attributes() & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) != 0
    }
    #[cfg(not(windows))]
    {
        let _ = meta;
        false
    }
}

fn collect(dir: &Path, min_size: u64, by_size: &mut HashMap<u64, Vec<PathBuf>>, p: &Progress) {
    if p.cancel.load(Ordering::Relaxed) {
        return;
    }
    let Ok(read) = fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else { continue };
        let ft = meta.file_type();
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            if !skip_dir(&entry.file_name().to_string_lossy()) {
                collect(&path, min_size, by_size, p);
            }
        } else if meta.len() >= min_size && !cloud_only(&meta) {
            p.files.fetch_add(1, Ordering::Relaxed);
            by_size.entry(meta.len()).or_default().push(path);
        }
    }
}

const CHUNK: usize = 64 * 1024;

/// Hash do começo e do fim do arquivo: separa rápido os que já diferem.
fn quick_hash(path: &Path, size: u64) -> io::Result<u64> {
    let mut f = File::open(path)?;
    let mut h = DefaultHasher::new();
    let mut buf = vec![0u8; CHUNK];
    let n = f.read(&mut buf)?;
    h.write(&buf[..n]);
    if size > (2 * CHUNK) as u64 {
        f.seek(SeekFrom::End(-(CHUNK as i64)))?;
        let n = f.read(&mut buf)?;
        h.write(&buf[..n]);
    }
    Ok(h.finish())
}

fn full_hash(path: &Path) -> io::Result<u64> {
    let mut f = File::open(path)?;
    let mut h = DefaultHasher::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.write(&buf[..n]);
    }
    Ok(h.finish())
}

fn same_content(a: &Path, b: &Path) -> io::Result<bool> {
    let (mut fa, mut fb) = (File::open(a)?, File::open(b)?);
    let (mut ba, mut bb) = (vec![0u8; 1024 * 1024], vec![0u8; 1024 * 1024]);
    loop {
        let na = fa.read(&mut ba)?;
        let mut nb = 0;
        while nb < na {
            let r = fb.read(&mut bb[nb..na])?;
            if r == 0 {
                break;
            }
            nb += r;
        }
        if na != nb || ba[..na] != bb[..nb] {
            return Ok(false);
        }
        if na == 0 {
            return Ok(true);
        }
    }
}

/// Agrupa `paths` (todos do mesmo tamanho) por uma chave; descarta os que
/// ficam sozinhos e os que não puderam ser lidos.
fn split_by(paths: Vec<PathBuf>, key: impl Fn(&Path) -> io::Result<u64>, p: &Progress) -> Vec<Vec<PathBuf>> {
    let mut m: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for path in paths {
        if p.cancel.load(Ordering::Relaxed) {
            return Vec::new();
        }
        if let Ok(k) = key(&path) {
            m.entry(k).or_default().push(path);
        }
        p.hashed.fetch_add(1, Ordering::Relaxed);
    }
    m.into_values().filter(|v| v.len() > 1).collect()
}

/// Procura duplicados em `roots` com pelo menos `min_size` bytes.
pub fn find(roots: &[PathBuf], min_size: u64, p: &Progress) -> Vec<Group> {
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for root in roots {
        collect(root, min_size.max(1), &mut by_size, p);
    }
    let mut groups = Vec::new();
    for (size, paths) in by_size {
        if paths.len() < 2 {
            continue;
        }
        for quick in split_by(paths, |f| quick_hash(f, size), p) {
            let candidates = if size as usize > 2 * CHUNK {
                split_by(quick, full_hash, p)
            } else {
                vec![quick]
            };
            for group in candidates {
                // Confirma byte a byte contra o primeiro.
                let first = group[0].clone();
                let mut same = vec![first.clone()];
                same.extend(group.into_iter().skip(1).filter(|f| same_content(&first, f).unwrap_or(false)));
                if same.len() > 1 {
                    let mut paths: Vec<String> = same.iter().map(|f| f.display().to_string()).collect();
                    paths.sort();
                    groups.push(Group {
                        size,
                        wasted: size * (paths.len() as u64 - 1),
                        paths,
                    });
                }
            }
        }
    }
    groups.sort_by(|a, b| b.wasted.cmp(&a.wasted).then_with(|| a.paths.cmp(&b.paths)));
    groups.truncate(1000);
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, data: &[u8]) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, data).unwrap();
    }

    #[test]
    fn finds_exact_duplicates_only() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        let big: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let mut almost = big.clone();
        almost[150_000] ^= 1; // mesmo tamanho, começo e fim iguais
        write(&r.join("a/video.mp4"), &big);
        write(&r.join("b/copia.mp4"), &big);
        write(&r.join("c/quase.mp4"), &almost);
        write(&r.join("d/x.txt"), b"igual");
        write(&r.join("d/y.txt"), b"igual");
        write(&r.join("AppData/z.txt"), b"igual"); // pasta pulada
        write(&r.join("pequeno1"), b"a");
        write(&r.join("pequeno2"), b"b");

        let p = Progress::default();
        let g = find(&[r.to_path_buf()], 1, &p);
        assert_eq!(g.len(), 2, "{g:?}");
        assert_eq!(g[0].size, 300_000);
        assert_eq!(g[0].paths.len(), 2);
        assert!(g[0].paths.iter().any(|x| x.ends_with("video.mp4")));
        assert!(g[0].paths.iter().any(|x| x.ends_with("copia.mp4")));
        assert!(!g[0].paths.iter().any(|x| x.contains("quase")));
        assert_eq!(g[0].wasted, 300_000);
        assert_eq!(g[1].paths.len(), 2, "AppData fica de fora");
        assert!(p.snapshot().files >= 7);

        // Tamanho mínimo filtra os pequenos.
        assert_eq!(find(&[r.to_path_buf()], 1000, &Progress::default()).len(), 1);
    }

    #[test]
    fn cancel_stops_early() {
        let d = tempfile::tempdir().unwrap();
        write(&d.path().join("a"), b"x");
        write(&d.path().join("b"), b"x");
        let p = Progress::default();
        p.cancel.store(true, Ordering::Relaxed);
        assert!(find(&[d.path().to_path_buf()], 1, &p).is_empty());
    }
}
