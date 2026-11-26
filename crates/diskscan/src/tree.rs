use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::scan::RawDir;

const NO_PARENT: u32 = u32::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeKind {
    Dir,
    File,
    /// Agregado dos arquivos pequenos de um diretório.
    Others,
    /// Agregado sintético (só existe em [`NodeView`]) dos filhos que não
    /// couberam no limite `max_children` de uma consulta.
    Rest,
}

#[derive(Debug)]
pub(crate) struct Node {
    pub(crate) name: Box<str>,
    pub(crate) parent: u32,
    pub(crate) size: u64,
    pub(crate) file_count: u64,
    pub(crate) dir_count: u64,
    pub(crate) kind: NodeKind,
    pub(crate) children: Vec<u32>,
    pub(crate) error: bool,
    pub(crate) removed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub path: String,
    pub size: u64,
}

/// Espaço por extensão de arquivo ("" = sem extensão).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtStat {
    pub ext: String,
    pub size: u64,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStats {
    pub root: String,
    pub total_size: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub errors: u64,
    pub cancelled: bool,
    pub elapsed_ms: u64,
}

/// Visão serializável de um nó (e opcionalmente de seus descendentes) para o
/// frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeView {
    /// `None` apenas para nós `Rest` sintéticos.
    pub id: Option<u32>,
    pub name: String,
    pub size: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub kind: NodeKind,
    pub has_children: bool,
    pub error: bool,
    /// Para `Rest`: quantos itens foram agrupados.
    pub hidden_items: u32,
    pub children: Vec<NodeView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Crumb {
    pub id: u32,
    pub name: String,
}

/// Árvore de uso de disco resultante de uma varredura.
#[derive(Debug)]
pub struct Tree {
    pub(crate) root_path: PathBuf,
    pub(crate) nodes: Vec<Node>,
    pub(crate) top_files: Vec<FileInfo>,
    pub(crate) top_limit: usize,
    pub(crate) errors: u64,
    pub(crate) cancelled: bool,
    pub(crate) elapsed_ms: u64,
    /// Da varredura completa; não acompanha exclusões nem re-análises.
    pub(crate) extensions: Vec<ExtStat>,
}

impl Tree {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_raw(
        root_path: PathBuf,
        raw: RawDir,
        top_files: Vec<FileInfo>,
        top_limit: usize,
        errors: u64,
        cancelled: bool,
        elapsed_ms: u64,
        mut extensions: Vec<ExtStat>,
    ) -> Self {
        extensions.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.ext.cmp(&b.ext)));
        let mut tree = Tree {
            root_path,
            nodes: Vec::new(),
            top_files,
            top_limit,
            errors,
            cancelled,
            elapsed_ms,
            extensions,
        };
        tree.flatten(raw, NO_PARENT);
        tree
    }

    fn push(&mut self, node: Node) -> u32 {
        let id = self.nodes.len() as u32;
        self.nodes.push(node);
        id
    }

    fn flatten(&mut self, raw: RawDir, parent: u32) -> u32 {
        let id = self.push(Node {
            name: raw.name.into_boxed_str(),
            parent,
            size: 0,
            file_count: 0,
            dir_count: 0,
            kind: NodeKind::Dir,
            children: Vec::new(),
            error: raw.error,
            removed: false,
        });
        let mut children = Vec::with_capacity(raw.dirs.len() + raw.files.len() + 1);
        let (mut size, mut files, mut dirs) = (0u64, 0u64, 0u64);

        for d in raw.dirs {
            let c = self.flatten(d, id);
            let n = &self.nodes[c as usize];
            size += n.size;
            files += n.file_count;
            dirs += n.dir_count + 1;
            children.push(c);
        }
        for (name, fsize) in raw.files {
            size += fsize;
            files += 1;
            children.push(self.push(Node {
                name: name.into_boxed_str(),
                parent: id,
                size: fsize,
                file_count: 1,
                dir_count: 0,
                kind: NodeKind::File,
                children: Vec::new(),
                error: false,
                removed: false,
            }));
        }
        if raw.small_count > 0 {
            size += raw.small_size;
            files += raw.small_count;
            children.push(self.push(Node {
                name: "".into(),
                parent: id,
                size: raw.small_size,
                file_count: raw.small_count,
                dir_count: 0,
                kind: NodeKind::Others,
                children: Vec::new(),
                error: false,
                removed: false,
            }));
        }

        let nodes = &self.nodes;
        children.sort_by(|a, b| nodes[*b as usize].size.cmp(&nodes[*a as usize].size));
        let node = &mut self.nodes[id as usize];
        node.children = children;
        node.size = size;
        node.file_count = files;
        node.dir_count = dirs;
        id
    }

    /// Espaço por extensão, maior primeiro.
    pub fn extensions(&self) -> &[ExtStat] {
        &self.extensions
    }

    pub fn root_id(&self) -> u32 {
        0
    }

    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    fn get(&self, id: u32) -> Option<&Node> {
        self.nodes.get(id as usize).filter(|n| !n.removed)
    }

    pub fn exists(&self, id: u32) -> bool {
        self.get(id).is_some()
    }

    pub fn kind(&self, id: u32) -> Option<NodeKind> {
        self.get(id).map(|n| n.kind)
    }

    pub fn stats(&self) -> ScanStats {
        let root = &self.nodes[0];
        ScanStats {
            root: self.root_path.to_string_lossy().into_owned(),
            total_size: root.size,
            file_count: root.file_count,
            dir_count: root.dir_count,
            errors: self.errors,
            cancelled: self.cancelled,
            elapsed_ms: self.elapsed_ms,
        }
    }

    pub fn top_files(&self, limit: usize) -> &[FileInfo] {
        &self.top_files[..limit.min(self.top_files.len())]
    }

    /// Caminho absoluto do nó. Para `Others`, devolve o diretório que o contém.
    pub fn path_of(&self, id: u32) -> Option<PathBuf> {
        self.get(id)?;
        let mut names = Vec::new();
        let mut cur = id;
        while cur != 0 {
            let n = &self.nodes[cur as usize];
            if n.kind != NodeKind::Others {
                names.push(&*n.name);
            }
            cur = n.parent;
        }
        let mut p = self.root_path.clone();
        for name in names.iter().rev() {
            p.push(name);
        }
        Some(p)
    }

    /// Ancestrais da raiz até o próprio nó (inclusive).
    pub fn ancestors(&self, id: u32) -> Vec<Crumb> {
        let mut out = Vec::new();
        if self.get(id).is_none() {
            return out;
        }
        let mut cur = id;
        loop {
            let n = &self.nodes[cur as usize];
            out.push(Crumb {
                id: cur,
                name: n.name.to_string(),
            });
            if n.parent == NO_PARENT {
                break;
            }
            cur = n.parent;
        }
        out.reverse();
        out
    }

    /// Visão do nó com `depth` níveis de descendentes. Em cada nível, só os
    /// `max_children` maiores filhos com tamanho >= `min_size` aparecem; o
    /// restante vira um nó `Rest`.
    pub fn view(&self, id: u32, depth: u32, max_children: usize, min_size: u64) -> Option<NodeView> {
        let n = self.get(id)?;
        let mut children = Vec::new();
        if depth > 0 {
            let max = max_children.max(1);
            // Filhos já estão ordenados por tamanho decrescente.
            let shown = n
                .children
                .iter()
                .take(max)
                .take_while(|&&c| self.nodes[c as usize].size >= min_size)
                .count();
            for &c in &n.children[..shown] {
                if let Some(v) = self.view(c, depth - 1, max_children, min_size) {
                    children.push(v);
                }
            }
            if n.children.len() > shown {
                let rest = &n.children[shown..];
                let (mut size, mut files, mut dirs) = (0, 0, 0);
                for &c in rest {
                    let cn = &self.nodes[c as usize];
                    size += cn.size;
                    files += cn.file_count;
                    dirs += cn.dir_count + u64::from(cn.kind == NodeKind::Dir);
                }
                children.push(NodeView {
                    id: None,
                    name: String::new(),
                    size,
                    file_count: files,
                    dir_count: dirs,
                    kind: NodeKind::Rest,
                    has_children: false,
                    error: false,
                    hidden_items: rest.len() as u32,
                    children: Vec::new(),
                });
            }
        }
        Some(NodeView {
            id: Some(id),
            name: n.name.to_string(),
            size: n.size,
            file_count: n.file_count,
            dir_count: n.dir_count,
            kind: n.kind,
            has_children: !n.children.is_empty(),
            error: n.error,
            hidden_items: 0,
            children,
        })
    }

    /// Ajusta tamanhos/contagens de `from` até a raiz e reordena os filhos de
    /// cada ancestral, já que a ordem por tamanho pode ter mudado.
    fn propagate(&mut self, from: u32, d_size: i128, d_files: i128, d_dirs: i128) {
        let mut cur = from;
        while cur != NO_PARENT {
            let n = &mut self.nodes[cur as usize];
            n.size = (n.size as i128 + d_size).max(0) as u64;
            n.file_count = (n.file_count as i128 + d_files).max(0) as u64;
            n.dir_count = (n.dir_count as i128 + d_dirs).max(0) as u64;
            let mut children = std::mem::take(&mut n.children);
            let nodes = &self.nodes;
            children.sort_by(|a, b| nodes[*b as usize].size.cmp(&nodes[*a as usize].size));
            let n = &mut self.nodes[cur as usize];
            n.children = children;
            cur = n.parent;
        }
    }

    fn drop_top_files_under(&mut self, path: &Path) {
        self.top_files.retain(|f| !Path::new(&f.path).starts_with(path));
    }

    /// Remove o nó da árvore (após ter sido apagado do disco). Devolve os
    /// bytes liberados. A raiz e nós `Others` não podem ser removidos.
    pub fn remove(&mut self, id: u32) -> Option<u64> {
        if id == 0 {
            return None;
        }
        let path = self.path_of(id)?;
        let n = &self.nodes[id as usize];
        if n.kind == NodeKind::Others {
            return None;
        }
        let (size, files, dirs, parent) = (
            n.size,
            n.file_count,
            n.dir_count + u64::from(n.kind == NodeKind::Dir),
            n.parent,
        );
        self.nodes[id as usize].removed = true;
        self.nodes[parent as usize].children.retain(|&c| c != id);
        self.propagate(parent, -(size as i128), -(files as i128), -(dirs as i128));
        self.drop_top_files_under(&path);
        Some(size)
    }

    /// Tamanho registrado de um arquivo da lista de maiores arquivos.
    pub fn top_file_size(&self, path: &Path) -> Option<u64> {
        self.top_files
            .iter()
            .find(|f| Path::new(&f.path) == path)
            .map(|f| f.size)
    }

    /// Remove um item pelo caminho (após ter sido apagado do disco). Funciona
    /// também para arquivos pequenos que não têm nó próprio porque foram
    /// agrupados no nó `Others` da pasta: nesse caso o tamanho vem da lista de
    /// maiores arquivos e é descontado do agrupamento.
    pub fn remove_path(&mut self, path: &Path) -> Option<u64> {
        if let Some(id) = self.find(path) {
            return self.remove(id);
        }
        let size = self.top_file_size(path)?;
        let parent = self.find(path.parent()?)?;
        let others = *self.nodes[parent as usize]
            .children
            .iter()
            .find(|&&c| self.nodes[c as usize].kind == NodeKind::Others)?;
        let o = &mut self.nodes[others as usize];
        let size = size.min(o.size);
        o.size -= size;
        o.file_count = o.file_count.saturating_sub(1);
        if o.file_count == 0 {
            o.removed = true;
            self.nodes[parent as usize].children.retain(|&c| c != others);
        }
        self.propagate(parent, -(size as i128), -1, 0);
        self.top_files.retain(|f| Path::new(&f.path) != path);
        Some(size)
    }

    /// Substitui o diretório `id` pelo resultado de uma nova varredura dele
    /// (`sub`). Usado para "re-varrer" só uma pasta.
    pub fn replace_subtree(&mut self, id: u32, sub: Tree) -> Option<()> {
        let old = self.get(id)?;
        if old.kind != NodeKind::Dir {
            return None;
        }
        let path = self.path_of(id)?;
        if id == 0 {
            *self = sub;
            return Some(());
        }
        let (old_size, old_files, old_dirs, parent) =
            (old.size, old.file_count, old.dir_count, old.parent);
        let old_name = old.name.clone();

        let offset = self.nodes.len() as u32;
        let new_root = offset;
        for (i, mut n) in sub.nodes.into_iter().enumerate() {
            if i == 0 {
                n.parent = parent;
                n.name = old_name.clone();
            } else {
                n.parent += offset;
            }
            for c in &mut n.children {
                *c += offset;
            }
            self.nodes.push(n);
        }
        self.nodes[id as usize].removed = true;
        for c in &mut self.nodes[parent as usize].children {
            if *c == id {
                *c = new_root;
            }
        }
        let new = &self.nodes[new_root as usize];
        let (ds, df, dd) = (
            new.size as i128 - old_size as i128,
            new.file_count as i128 - old_files as i128,
            new.dir_count as i128 - old_dirs as i128,
        );
        self.propagate(parent, ds, df, dd);

        self.drop_top_files_under(&path);
        self.top_files.extend(sub.top_files);
        self.top_files.sort_by_key(|f| std::cmp::Reverse(f.size));
        self.top_files.truncate(self.top_limit);
        self.errors += sub.errors;
        Some(())
    }

    /// Encontra o id de um caminho absoluto dentro da árvore.
    pub fn find(&self, path: &Path) -> Option<u32> {
        let rel = path.strip_prefix(&self.root_path).ok()?;
        let mut cur = 0u32;
        for comp in rel.components() {
            let name = comp.as_os_str().to_string_lossy();
            cur = *self.nodes[cur as usize].children.iter().find(|&&c| {
                let n = &self.nodes[c as usize];
                n.kind != NodeKind::Others && *n.name == *name
            })?;
        }
        Some(cur)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{scan, Progress, ScanOptions};
    use std::fs;

    fn write(path: &Path, size: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![0u8; size]).unwrap();
    }

    fn fixture() -> (tempfile::TempDir, Tree) {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        write(&r.join("big.bin"), 5000);
        write(&r.join("a/x.bin"), 3000);
        write(&r.join("a/small1"), 10);
        write(&r.join("a/small2"), 20);
        write(&r.join("a/deep/y.bin"), 2000);
        write(&r.join("b/z.bin"), 1000);
        fs::create_dir_all(r.join("empty")).unwrap();
        let opts = ScanOptions {
            min_file_size: 100,
            top_files: 3,
        };
        let tree = scan(r, &opts, &Progress::new()).unwrap();
        (dir, tree)
    }

    #[test]
    fn totals_and_ordering() {
        let (_d, t) = fixture();
        let s = t.stats();
        assert_eq!(s.total_size, 5000 + 3000 + 30 + 2000 + 1000);
        assert_eq!(s.file_count, 6);
        assert_eq!(s.dir_count, 4); // a, a/deep, b, empty

        let v = t.view(0, 2, 100, 0).unwrap();
        let names: Vec<_> = v.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["a", "big.bin", "b", "empty"]);
        let a = &v.children[0];
        assert_eq!(a.size, 5030);
        // a: x.bin(3000), deep(2000), outros(30)
        assert_eq!(a.children.len(), 3);
        assert_eq!(a.children[2].kind, NodeKind::Others);
        assert_eq!(a.children[2].file_count, 2);
    }

    #[test]
    fn min_size_aggregation() {
        let (_d, t) = fixture();
        let v = t.view(0, 1, 100, 2000).unwrap();
        let names: Vec<_> = v.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["a", "big.bin", ""]);
        assert_eq!(v.children[2].kind, NodeKind::Rest);
        assert_eq!(v.children[2].hidden_items, 2); // b + empty
    }

    #[test]
    fn top_files_are_sorted_and_limited() {
        let (_d, t) = fixture();
        let sizes: Vec<_> = t.top_files(10).iter().map(|f| f.size).collect();
        assert_eq!(sizes, [5000, 3000, 2000]);
    }

    #[test]
    fn rest_aggregation() {
        let (_d, t) = fixture();
        let v = t.view(0, 1, 2, 0).unwrap();
        assert_eq!(v.children.len(), 3);
        let rest = &v.children[2];
        assert_eq!(rest.kind, NodeKind::Rest);
        assert_eq!(rest.hidden_items, 2);
        assert_eq!(rest.size, 1000);
    }

    #[test]
    fn path_find_and_ancestors() {
        let (d, t) = fixture();
        let deep = d.path().join("a").join("deep");
        let id = t.find(&deep).unwrap();
        assert_eq!(t.path_of(id).unwrap(), deep);
        let crumbs: Vec<_> = t.ancestors(id).into_iter().map(|c| c.name).collect();
        assert_eq!(crumbs[1..], ["a", "deep"]);
    }

    #[test]
    fn remove_updates_ancestors() {
        let (d, mut t) = fixture();
        let a = t.find(&d.path().join("a")).unwrap();
        let x = t.find(&d.path().join("a/x.bin")).unwrap();
        assert_eq!(t.remove(x), Some(3000));
        assert_eq!(t.stats().total_size, 8030);
        assert_eq!(t.stats().file_count, 5);
        assert!(!t.exists(x));
        // big.bin (5000) agora é maior que a (2030)
        let v = t.view(0, 1, 100, 0).unwrap();
        assert_eq!(v.children[0].name, "big.bin");
        assert_eq!(t.view(a, 0, 0, 0).unwrap().size, 2030);
        assert!(t.top_files(10).iter().all(|f| f.size != 3000));
        assert_eq!(t.remove(0), None);
    }

    #[test]
    fn rescan_subtree() {
        let (d, mut t) = fixture();
        let b_path = d.path().join("b");
        write(&b_path.join("new.bin"), 4000);
        let b = t.find(&b_path).unwrap();
        let opts = ScanOptions {
            min_file_size: 100,
            top_files: 3,
        };
        let sub = scan(&b_path, &opts, &Progress::new()).unwrap();
        t.replace_subtree(b, sub).unwrap();
        assert_eq!(t.stats().total_size, 11030 + 4000);
        let nb = t.find(&b_path).unwrap();
        assert_eq!(t.view(nb, 0, 0, 0).unwrap().name, "b");
        assert_eq!(t.path_of(nb).unwrap(), b_path);
        let sizes: Vec<_> = t.top_files(10).iter().map(|f| f.size).collect();
        assert_eq!(sizes, [5000, 4000, 3000]);
    }
}
