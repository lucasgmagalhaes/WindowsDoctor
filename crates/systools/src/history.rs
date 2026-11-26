//! Histórico das alterações feitas pelo app, com o necessário para desfazer.
//! Fica em `%LOCALAPPDATA%\<app>\history.json`.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const MAX_ENTRIES: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Undo {
    Tweak { id: String, apply: bool },
    Startup { id: String, enabled: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: u64,
    /// Milissegundos desde 1970.
    pub time_ms: u64,
    /// "tweak", "startup", "cleanup", "delete", "restore", "repair"...
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub undo: Option<Undo>,
    #[serde(default)]
    pub undone: bool,
}

pub struct History {
    path: Option<PathBuf>,
    entries: Mutex<Vec<Entry>>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

impl History {
    /// Carrega o arquivo; se não existir ou estiver corrompido, começa vazio.
    pub fn load(path: Option<PathBuf>) -> Self {
        let entries = path
            .as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        History {
            path,
            entries: Mutex::new(entries),
        }
    }

    fn save(&self, entries: &[Entry]) {
        let Some(path) = &self.path else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(entries) {
            // Grava num temporário e renomeia para não corromper o arquivo.
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, path);
            }
        }
    }

    pub fn record(&self, kind: &str, title: impl Into<String>, detail: impl Into<String>, undo: Option<Undo>) {
        let mut entries = self.entries.lock().unwrap();
        let mut id = now_ms();
        if let Some(last) = entries.last() {
            id = id.max(last.id + 1);
        }
        entries.push(Entry {
            id,
            time_ms: now_ms(),
            kind: kind.into(),
            title: title.into(),
            detail: detail.into(),
            undo,
            undone: false,
        });
        let excess = entries.len().saturating_sub(MAX_ENTRIES);
        entries.drain(..excess);
        self.save(&entries);
    }

    /// Mais recentes primeiro.
    pub fn list(&self) -> Vec<Entry> {
        let mut v = self.entries.lock().unwrap().clone();
        v.reverse();
        v
    }

    pub fn get(&self, id: u64) -> Option<Entry> {
        self.entries.lock().unwrap().iter().find(|e| e.id == id).cloned()
    }

    pub fn mark_undone(&self, id: u64) {
        let mut entries = self.entries.lock().unwrap();
        if let Some(e) = entries.iter_mut().find(|e| e.id == id) {
            e.undone = true;
        }
        self.save(&entries);
    }

    pub fn clear(&self) {
        let mut entries = self.entries.lock().unwrap();
        entries.clear();
        self.save(&entries);
    }
}

/// Tamanho legível para textos do histórico ("1,5 GB").
pub fn fmt_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", UNITS[i]).replace('.', ",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_persists_and_marks_undone() {
        let dir = std::env::temp_dir().join(format!("wd-history-{}", now_ms()));
        let path = dir.join("history.json");
        let h = History::load(Some(path.clone()));
        h.record("tweak", "Desativar MPO", "", Some(Undo::Tweak { id: "disable_mpo".into(), apply: false }));
        h.record("cleanup", "Limpeza", "1 GB", None);
        let list = h.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].kind, "cleanup", "mais recente primeiro");
        assert!(list[0].id > list[1].id);

        h.mark_undone(list[1].id);
        let again = History::load(Some(path));
        let e = again.get(list[1].id).unwrap();
        assert!(e.undone);
        assert_eq!(e.undo, Some(Undo::Tweak { id: "disable_mpo".into(), apply: false }));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn formats_bytes() {
        assert_eq!(fmt_bytes(512), "512 B");
        assert_eq!(fmt_bytes(1536 * 1024 * 1024), "1,5 GB");
    }

    #[test]
    fn keeps_only_the_latest_entries() {
        let h = History::load(None);
        for i in 0..MAX_ENTRIES + 10 {
            h.record("x", format!("{i}"), "", None);
        }
        let list = h.list();
        assert_eq!(list.len(), MAX_ENTRIES);
        assert_eq!(list[0].title, format!("{}", MAX_ENTRIES + 9));
    }
}
