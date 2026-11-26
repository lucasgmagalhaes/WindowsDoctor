//! Formato binário para salvar e reabrir uma varredura.
//!
//! Tudo em little-endian; textos são UTF-8 com o tamanho em u32 na frente.
//! A árvore é gravada exatamente como está (inclusive exclusões já feitas).

use std::io::{self, Read, Write};
use std::path::PathBuf;

use crate::tree::{ExtStat, FileInfo, Node, NodeKind, Tree};

const MAGIC: &[u8; 8] = b"WDSCAN\x00\x01";

struct W<T: Write>(T);

impl<T: Write> W<T> {
    fn u8(&mut self, v: u8) -> io::Result<()> {
        self.0.write_all(&[v])
    }
    fn u32(&mut self, v: u32) -> io::Result<()> {
        self.0.write_all(&v.to_le_bytes())
    }
    fn u64(&mut self, v: u64) -> io::Result<()> {
        self.0.write_all(&v.to_le_bytes())
    }
    fn str(&mut self, s: &str) -> io::Result<()> {
        self.u32(s.len() as u32)?;
        self.0.write_all(s.as_bytes())
    }
}

struct R<T: Read>(T);

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

impl<T: Read> R<T> {
    fn bytes<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        let mut b = [0u8; N];
        self.0.read_exact(&mut b)?;
        Ok(b)
    }
    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.bytes::<1>()?[0])
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.bytes()?))
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.bytes()?))
    }
    fn str(&mut self) -> io::Result<String> {
        let len = self.u32()? as usize;
        if len > 64 * 1024 * 1024 {
            return Err(bad("texto grande demais"));
        }
        let mut b = vec![0u8; len];
        self.0.read_exact(&mut b)?;
        String::from_utf8(b).map_err(|_| bad("texto inválido"))
    }
    /// Quantidade de itens, limitada para não alocar absurdos com arquivo corrompido.
    fn count(&mut self, max: u32) -> io::Result<usize> {
        let n = self.u32()?;
        if n > max {
            return Err(bad("arquivo corrompido"));
        }
        Ok(n as usize)
    }
}

fn kind_code(k: NodeKind) -> u8 {
    match k {
        NodeKind::Dir => 0,
        NodeKind::File => 1,
        NodeKind::Others => 2,
        NodeKind::Rest => 3,
    }
}

fn kind_from(c: u8) -> io::Result<NodeKind> {
    Ok(match c {
        0 => NodeKind::Dir,
        1 => NodeKind::File,
        2 => NodeKind::Others,
        3 => NodeKind::Rest,
        _ => return Err(bad("tipo de nó desconhecido")),
    })
}

impl Tree {
    /// Grava a varredura. Use um `BufWriter`.
    pub fn save(&self, out: impl Write) -> io::Result<()> {
        let mut w = W(out);
        w.0.write_all(MAGIC)?;
        w.str(&self.root_path.to_string_lossy())?;
        w.u64(self.top_limit as u64)?;
        w.u64(self.errors)?;
        w.u8(u8::from(self.cancelled))?;
        w.u64(self.elapsed_ms)?;
        w.u32(self.top_files.len() as u32)?;
        for f in &self.top_files {
            w.str(&f.path)?;
            w.u64(f.size)?;
        }
        w.u32(self.extensions.len() as u32)?;
        for e in &self.extensions {
            w.str(&e.ext)?;
            w.u64(e.size)?;
            w.u64(e.count)?;
        }
        w.u32(self.nodes.len() as u32)?;
        for n in &self.nodes {
            w.str(&n.name)?;
            w.u32(n.parent)?;
            w.u64(n.size)?;
            w.u64(n.file_count)?;
            w.u64(n.dir_count)?;
            w.u8(kind_code(n.kind))?;
            w.u8(u8::from(n.error) | (u8::from(n.removed) << 1))?;
            w.u32(n.children.len() as u32)?;
            for c in &n.children {
                w.u32(*c)?;
            }
        }
        w.0.flush()
    }

    /// Lê uma varredura salva com [`Tree::save`]. Use um `BufReader`.
    pub fn load(input: impl Read) -> io::Result<Tree> {
        let mut r = R(input);
        if &r.bytes::<8>()? != MAGIC {
            return Err(bad("não é um arquivo de varredura do WindowsDoctor (ou é de outra versão)"));
        }
        let root_path = PathBuf::from(r.str()?);
        let top_limit = r.u64()? as usize;
        let errors = r.u64()?;
        let cancelled = r.u8()? != 0;
        let elapsed_ms = r.u64()?;
        let n = r.count(10_000_000)?;
        let mut top_files = Vec::with_capacity(n);
        for _ in 0..n {
            top_files.push(FileInfo { path: r.str()?, size: r.u64()? });
        }
        let n = r.count(10_000_000)?;
        let mut extensions = Vec::with_capacity(n);
        for _ in 0..n {
            extensions.push(ExtStat { ext: r.str()?, size: r.u64()?, count: r.u64()? });
        }
        let total = r.count(200_000_000)?;
        let mut nodes = Vec::with_capacity(total.min(1 << 20));
        for _ in 0..total {
            let name = r.str()?.into_boxed_str();
            let parent = r.u32()?;
            let size = r.u64()?;
            let file_count = r.u64()?;
            let dir_count = r.u64()?;
            let kind = kind_from(r.u8()?)?;
            let flags = r.u8()?;
            let nc = r.count(total as u32)?;
            let mut children = Vec::with_capacity(nc);
            for _ in 0..nc {
                let c = r.u32()?;
                if c as usize >= total {
                    return Err(bad("arquivo corrompido"));
                }
                children.push(c);
            }
            nodes.push(Node {
                name,
                parent,
                size,
                file_count,
                dir_count,
                kind,
                children,
                error: flags & 1 != 0,
                removed: flags & 2 != 0,
            });
        }
        if nodes.is_empty() {
            return Err(bad("varredura vazia"));
        }
        Ok(Tree {
            root_path,
            nodes,
            top_files,
            top_limit,
            errors,
            cancelled,
            elapsed_ms,
            extensions,
        })
    }
}
