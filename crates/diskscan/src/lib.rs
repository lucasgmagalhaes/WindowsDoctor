//! Varredura paralela de diretórios e árvore de uso de disco.
//!
//! O fluxo é:
//! 1. [`scan`] percorre o diretório em paralelo (rayon) montando uma árvore
//!    temporária de baixo para cima;
//! 2. a árvore é achatada em um [`Tree`] (arena de nós com ids `u32`), que é
//!    barato de consultar, navegar e atualizar após exclusões.
//!
//! Arquivos menores que [`ScanOptions::min_file_size`] não viram nós
//! individuais: são somados em um nó "outros arquivos" por diretório. Isso
//! mantém o uso de memória baixo em partições com milhões de arquivos.

mod scan;
mod tree;

pub use scan::{scan, Progress, ProgressSnapshot, ScanOptions};
pub use tree::{Crumb, ExtStat, FileInfo, NodeKind, NodeView, ScanStats, Tree};
