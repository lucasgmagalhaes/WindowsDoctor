//! Resumo das pastas grandes de uma varredura e comparação entre duas.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::tree::{NodeKind, Tree};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub root: String,
    pub time_ms: u64,
    pub total: u64,
    /// Caminho → tamanho das pastas com pelo menos `min_size`.
    pub dirs: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub path: String,
    pub before: u64,
    pub after: u64,
    pub delta: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub previous_time_ms: u64,
    pub previous_total: u64,
    pub current_total: u64,
    pub changes: Vec<Change>,
}

impl Tree {
    /// Pastas com pelo menos `min_size` bytes até `max_depth` níveis.
    pub fn snapshot(&self, time_ms: u64, min_size: u64, max_depth: u32) -> Snapshot {
        let mut dirs = HashMap::new();
        let mut stack = vec![(self.root_id(), self.root_path.clone(), 0u32)];
        while let Some((id, path, depth)) = stack.pop() {
            let n = &self.nodes[id as usize];
            if n.size < min_size {
                continue;
            }
            dirs.insert(path.to_string_lossy().into_owned(), n.size);
            if depth >= max_depth {
                continue;
            }
            for &c in &n.children {
                let child = &self.nodes[c as usize];
                if child.kind == NodeKind::Dir && !child.removed && child.size >= min_size {
                    let mut p = PathBuf::from(&path);
                    p.push(&*child.name);
                    stack.push((c, p, depth + 1));
                }
            }
        }
        Snapshot {
            root: self.root_path.to_string_lossy().into_owned(),
            time_ms,
            total: self.nodes[self.root_id() as usize].size,
            dirs,
        }
    }
}

fn is_descendant(child: &str, parent: &str) -> bool {
    child.len() > parent.len()
        && child.starts_with(parent)
        && matches!(child.as_bytes()[parent.len()], b'\\' | b'/')
        || (parent.ends_with(['\\', '/']) && child.len() > parent.len() && child.starts_with(parent))
}

/// Pastas que mudaram pelo menos `min_delta` bytes. Uma pasta é omitida quando
/// uma subpasta explica 80% ou mais da mudança dela (mostra a mais específica).
pub fn compare(old: &Snapshot, new: &Snapshot, min_delta: u64, limit: usize) -> Comparison {
    let mut all: Vec<Change> = Vec::new();
    for (path, &after) in &new.dirs {
        let before = old.dirs.get(path).copied().unwrap_or(0);
        all.push(Change { path: path.clone(), before, after, delta: after as i64 - before as i64 });
    }
    for (path, &before) in &old.dirs {
        if !new.dirs.contains_key(path) {
            // Pasta que sumiu ou ficou abaixo do mínimo: tratada como zero.
            all.push(Change { path: path.clone(), before, after: 0, delta: -(before as i64) });
        }
    }
    all.retain(|c| c.delta.unsigned_abs() >= min_delta);
    let candidates = all.clone();
    all.retain(|p| {
        !candidates.iter().any(|c| {
            c.path != p.path
                && is_descendant(&c.path, &p.path)
                && c.delta.signum() == p.delta.signum()
                && c.delta.unsigned_abs() * 10 >= p.delta.unsigned_abs() * 8
        })
    });
    all.sort_by(|a, b| b.delta.unsigned_abs().cmp(&a.delta.unsigned_abs()).then_with(|| a.path.cmp(&b.path)));
    all.truncate(limit);
    Comparison {
        previous_time_ms: old.time_ms,
        previous_total: old.total,
        current_total: new.total,
        changes: all,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(total: u64, dirs: &[(&str, u64)]) -> Snapshot {
        Snapshot {
            root: "C:\\".into(),
            time_ms: 1,
            total,
            dirs: dirs.iter().map(|(p, s)| (p.to_string(), *s)).collect(),
        }
    }

    const MB: u64 = 1024 * 1024;

    #[test]
    fn shows_the_most_specific_folder_that_changed() {
        let old = snap(100_000 * MB, &[
            ("C:\\", 100_000 * MB),
            ("C:\\Users", 40_000 * MB),
            ("C:\\Users\\voce", 39_000 * MB),
            ("C:\\Users\\voce\\AppData\\Local\\Temp", 1_000 * MB),
            ("C:\\Games", 50_000 * MB),
        ]);
        let new = snap(108_000 * MB, &[
            ("C:\\", 108_000 * MB),
            ("C:\\Users", 49_000 * MB),
            ("C:\\Users\\voce", 48_000 * MB),
            ("C:\\Users\\voce\\AppData\\Local\\Temp", 10_000 * MB),
            ("C:\\Games", 49_000 * MB),
            ("C:\\Novo", 600 * MB),
        ]);
        let c = compare(&old, &new, 500 * MB, 50);
        let paths: Vec<&str> = c.changes.iter().map(|x| x.path.as_str()).collect();
        assert_eq!(paths, ["C:\\Users\\voce\\AppData\\Local\\Temp", "C:\\Games", "C:\\Novo"]);
        assert_eq!(c.changes[0].delta, 9_000 * MB as i64);
        assert_eq!(c.changes[1].delta, -(1_000 * MB as i64));
        assert_eq!(c.changes[2].before, 0);
        assert_eq!(c.current_total - c.previous_total, 8_000 * MB);
    }

    #[test]
    fn removed_folders_count_as_shrinking() {
        let old = snap(10, &[("C:\\", 2000 * MB), ("C:\\Velho", 900 * MB)]);
        let new = snap(10, &[("C:\\", 1100 * MB)]);
        let c = compare(&old, &new, 100 * MB, 50);
        assert_eq!(c.changes.len(), 1, "{:?}", c.changes);
        assert_eq!(c.changes[0].path, "C:\\Velho");
        assert_eq!(c.changes[0].after, 0);
    }

    #[test]
    fn descendant_check_respects_separators() {
        assert!(is_descendant("C:\\Users\\a", "C:\\Users"));
        assert!(!is_descendant("C:\\Users2", "C:\\Users"));
        assert!(is_descendant("C:\\Users", "C:\\"));
    }
}
