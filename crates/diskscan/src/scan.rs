use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use rayon::prelude::*;
use serde::Serialize;

use crate::tree::{FileInfo, Tree};

/// Opções de varredura.
#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Arquivos menores que isso (bytes) são agregados em um nó
    /// "outros arquivos" por diretório em vez de virarem nós próprios.
    pub min_file_size: u64,
    /// Quantos dos maiores arquivos da varredura guardar para a lista global.
    pub top_files: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            min_file_size: 1024 * 1024,
            top_files: 500,
        }
    }
}

/// Estado compartilhado de uma varredura em andamento. Pode ser lido de outra
/// thread para mostrar progresso, e permite cancelar.
#[derive(Default)]
pub struct Progress {
    files: AtomicU64,
    dirs: AtomicU64,
    bytes: AtomicU64,
    errors: AtomicU64,
    cancel: AtomicBool,
    current: Mutex<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressSnapshot {
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
    pub errors: u64,
    pub current: String,
}

impl Progress {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn snapshot(&self) -> ProgressSnapshot {
        ProgressSnapshot {
            files: self.files.load(Ordering::Relaxed),
            dirs: self.dirs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            current: self.current.lock().map(|s| s.clone()).unwrap_or_default(),
        }
    }

    fn set_current(&self, path: &Path) {
        // try_lock: o caminho atual é só informativo, não vale a pena disputar
        // o lock entre as threads de varredura.
        if let Ok(mut cur) = self.current.try_lock() {
            cur.clear();
            cur.push_str(&path.to_string_lossy());
        }
    }
}

/// Diretório já varrido, antes de ser achatado na arena.
pub(crate) struct RawDir {
    pub name: String,
    pub files: Vec<(String, u64)>,
    pub small_count: u64,
    pub small_size: u64,
    pub dirs: Vec<RawDir>,
    /// Não foi possível listar este diretório (acesso negado etc.).
    pub error: bool,
}

/// Coletor dos N maiores arquivos, com um limiar atômico para evitar pegar o
/// lock na imensa maioria dos arquivos (que são pequenos).
struct TopFiles {
    limit: usize,
    threshold: AtomicU64,
    heap: Mutex<BinaryHeap<Reverse<(u64, PathBuf)>>>,
}

impl TopFiles {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            threshold: AtomicU64::new(0),
            heap: Mutex::new(BinaryHeap::new()),
        }
    }

    fn offer(&self, size: u64, path: impl FnOnce() -> PathBuf) {
        if self.limit == 0 || size <= self.threshold.load(Ordering::Relaxed) {
            return;
        }
        let mut heap = self.heap.lock().unwrap();
        heap.push(Reverse((size, path())));
        if heap.len() > self.limit {
            heap.pop();
        }
        if heap.len() == self.limit {
            if let Some(Reverse((min, _))) = heap.peek() {
                self.threshold.store(*min, Ordering::Relaxed);
            }
        }
    }

    fn into_sorted(self) -> Vec<FileInfo> {
        let heap = self.heap.into_inner().unwrap();
        // into_sorted_vec de Reverse => ordem decrescente de tamanho.
        heap.into_sorted_vec()
            .into_iter()
            .map(|Reverse((size, path))| FileInfo {
                path: path.to_string_lossy().into_owned(),
                size,
            })
            .collect()
    }
}

struct Ctx<'a> {
    opts: &'a ScanOptions,
    progress: &'a Progress,
    top: TopFiles,
}

/// Tamanho que o arquivo realmente ocupa localmente. Arquivos "somente
/// online" (OneDrive etc.) reportam o tamanho lógico mas não ocupam disco.
fn local_size(meta: &fs::Metadata) -> u64 {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_OFFLINE: u32 = 0x1000;
        const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
        if meta.file_attributes() & (FILE_ATTRIBUTE_OFFLINE | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS)
            != 0
        {
            return 0;
        }
    }
    meta.len()
}

fn scan_dir(path: &Path, name: String, ctx: &Ctx) -> RawDir {
    let mut raw = RawDir {
        name,
        files: Vec::new(),
        small_count: 0,
        small_size: 0,
        dirs: Vec::new(),
        error: false,
    };
    if ctx.progress.is_cancelled() {
        return raw;
    }
    ctx.progress.dirs.fetch_add(1, Ordering::Relaxed);
    ctx.progress.set_current(path);

    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => {
            ctx.progress.errors.fetch_add(1, Ordering::Relaxed);
            raw.error = true;
            return raw;
        }
    };

    let mut subdirs: Vec<(PathBuf, String)> = Vec::new();
    let mut bytes_here = 0u64;
    let mut files_here = 0u64;

    for entry in entries {
        let Ok(entry) = entry else {
            ctx.progress.errors.fetch_add(1, Ordering::Relaxed);
            continue;
        };
        let Ok(ft) = entry.file_type() else {
            ctx.progress.errors.fetch_add(1, Ordering::Relaxed);
            continue;
        };
        // Symlinks e junctions (no Windows `is_symlink` cobre os dois) são
        // ignorados: evitam contagem dupla e loops infinitos.
        if ft.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if ft.is_dir() {
            subdirs.push((entry.path(), name));
            continue;
        }
        let size = match entry.metadata() {
            Ok(m) => local_size(&m),
            Err(_) => {
                ctx.progress.errors.fetch_add(1, Ordering::Relaxed);
                continue;
            }
        };
        files_here += 1;
        bytes_here += size;
        ctx.top.offer(size, || entry.path());
        if size >= ctx.opts.min_file_size {
            raw.files.push((name, size));
        } else {
            raw.small_count += 1;
            raw.small_size += size;
        }
    }
    ctx.progress.files.fetch_add(files_here, Ordering::Relaxed);
    ctx.progress.bytes.fetch_add(bytes_here, Ordering::Relaxed);

    raw.dirs = subdirs
        .into_par_iter()
        .map(|(p, n)| scan_dir(&p, n, ctx))
        .collect();
    raw
}

/// Varre `root` recursivamente e devolve a árvore de uso de disco.
///
/// Bloqueia até terminar (ou até `progress.cancel()` ser chamado, caso em que
/// a árvore parcial é devolvida e `stats.cancelled` fica `true`).
pub fn scan(root: &Path, opts: &ScanOptions, progress: &Progress) -> std::io::Result<Tree> {
    let meta = fs::metadata(root)?;
    if !meta.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "o caminho não é um diretório",
        ));
    }
    let started = Instant::now();
    let ctx = Ctx {
        opts,
        progress,
        top: TopFiles::new(opts.top_files),
    };

    // Pool próprio com pilha maior: a recursão acompanha a profundidade das
    // pastas, que no Windows pode passar de centenas de níveis.
    let pool = rayon::ThreadPoolBuilder::new()
        .stack_size(16 * 1024 * 1024)
        .build()
        .map_err(std::io::Error::other)?;
    let raw = pool.install(|| scan_dir(root, root.to_string_lossy().into_owned(), &ctx));

    let top = ctx.top.into_sorted();
    let snap = progress.snapshot();
    Ok(Tree::from_raw(
        root.to_path_buf(),
        raw,
        top,
        opts.top_files,
        snap.errors,
        progress.is_cancelled(),
        started.elapsed().as_millis() as u64,
    ))
}
