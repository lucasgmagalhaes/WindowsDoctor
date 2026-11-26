//! Move uma pasta para outro disco e deixa no lugar um link (junction no
//! Windows) para os programas continuarem achando os arquivos.
//!
//! Ordem segura: copia → confere → renomeia a original (falha se algo a usa)
//! → cria o link → apaga a original. Em qualquer falha antes do fim, desfaz.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    pub from: PathBuf,
    pub to: PathBuf,
    pub bytes: u64,
    /// A pasta original renomeada que não pôde ser apagada (apague depois).
    pub leftover: Option<PathBuf>,
}

/// Pastas que nunca podem ser movidas.
pub fn is_protected(p: &Path) -> bool {
    let s = p.to_string_lossy().to_lowercase().replace('/', "\\");
    let s = s.trim_end_matches('\\');
    // Raiz de unidade ("c:") ou do sistema de arquivos.
    let drive_root = s.len() == 2 && s.ends_with(':');
    if p.parent().is_none() || drive_root || s.is_empty() {
        return true;
    }
    let mut blocked: Vec<String> = ["windows", "program files", "program files (x86)", "programdata", "users", "boot", "recovery"]
        .iter()
        .map(|d| format!("c:\\{d}"))
        .collect();
    for var in ["SystemRoot", "USERPROFILE", "ProgramFiles", "ProgramFiles(x86)", "ProgramData", "APPDATA", "LOCALAPPDATA"] {
        if let Some(v) = std::env::var_os(var) {
            blocked.push(v.to_string_lossy().to_lowercase().trim_end_matches('\\').to_string());
        }
    }
    if let Some(win) = std::env::var_os("SystemRoot") {
        let w = win.to_string_lossy().to_lowercase();
        if s.starts_with(&format!("{}\\", w.trim_end_matches('\\'))) {
            return true;
        }
    }
    blocked.iter().any(|b| b == s) || s.ends_with("\\appdata") || s.ends_with("\\appdata\\local") || s.ends_with("\\appdata\\roaming")
}

fn size_of(dir: &Path) -> io::Result<u64> {
    let mut total = 0;
    for e in fs::read_dir(dir)? {
        let e = e?;
        let m = fs::symlink_metadata(e.path())?;
        if m.file_type().is_symlink() {
            return Err(io::Error::other(format!("{} contém um link; mova manualmente", dir.display())));
        }
        total += if m.is_dir() { size_of(&e.path())? } else { m.len() };
    }
    Ok(total)
}

fn copy_dir(src: &Path, dst: &Path, copied: &mut u64, on_progress: &mut dyn FnMut(u64)) -> io::Result<()> {
    fs::create_dir(dst)?;
    for e in fs::read_dir(src)? {
        let e = e?;
        let from = e.path();
        let to = dst.join(e.file_name());
        let m = fs::symlink_metadata(&from)?;
        if m.is_dir() {
            copy_dir(&from, &to, copied, on_progress)?;
        } else {
            *copied += fs::copy(&from, &to)?;
            on_progress(*copied);
        }
    }
    Ok(())
}

fn make_link(link: &Path, target: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        // Junctions não exigem administrador nem modo de desenvolvedor.
        let out = crate::shell::command("cmd.exe")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(target)
            .output()?;
        if !out.status.success() {
            return Err(io::Error::other(String::from_utf8_lossy(&out.stdout).trim().to_string()));
        }
        Ok(())
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = (link, target);
        Err(io::Error::other("sem suporte a links"))
    }
}

/// Move `src` para dentro de `dest_parent` e cria o link no lugar.
/// `on_progress(copiado, total)` é chamado durante a cópia.
pub fn move_with_link(src: &Path, dest_parent: &Path, mut on_progress: impl FnMut(u64, u64)) -> Result<Moved, String> {
    let meta = fs::symlink_metadata(src).map_err(|e| format!("{}: {e}", src.display()))?;
    if meta.file_type().is_symlink() {
        return Err("esta pasta já é um link".into());
    }
    if !meta.is_dir() {
        return Err("só é possível mover pastas".into());
    }
    if is_protected(src) {
        return Err("esta pasta do sistema não pode ser movida".into());
    }
    if !dest_parent.is_dir() {
        return Err("o destino não existe".into());
    }
    let name = src.file_name().ok_or("pasta inválida")?;
    let dest = dest_parent.join(name);
    if dest.exists() {
        return Err(format!("já existe {}", dest.display()));
    }
    let src_abs = fs::canonicalize(src).map_err(|e| e.to_string())?;
    let parent_abs = fs::canonicalize(dest_parent).map_err(|e| e.to_string())?;
    if parent_abs.starts_with(&src_abs) {
        return Err("o destino não pode ficar dentro da própria pasta".into());
    }

    let total = size_of(src).map_err(|e| e.to_string())?;
    let mut copied = 0u64;
    let cleanup = |msg: String| -> String {
        let _ = fs::remove_dir_all(&dest);
        msg
    };
    copy_dir(src, &dest, &mut copied, &mut |c| on_progress(c, total))
        .map_err(|e| cleanup(format!("falha ao copiar (nada foi alterado): {e}")))?;
    let check = size_of(&dest).map_err(|e| cleanup(e.to_string()))?;
    if check != total {
        return Err(cleanup(format!(
            "a cópia ficou com {check} bytes em vez de {total}; nada foi alterado"
        )));
    }

    let mut old = src.as_os_str().to_owned();
    old.push(".wd-movendo");
    let old = PathBuf::from(old);
    fs::rename(src, &old).map_err(|e| cleanup(format!("a pasta está em uso; feche os programas que a usam e tente de novo ({e})")))?;
    if let Err(e) = make_link(src, &dest) {
        let _ = fs::rename(&old, src);
        return Err(cleanup(format!("não foi possível criar o link: {e}")));
    }
    let leftover = fs::remove_dir_all(&old).err().map(|_| old);
    Ok(Moved {
        from: src.to_path_buf(),
        to: dest,
        bytes: total,
        leftover,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn moves_and_leaves_a_link() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("origem/Jogos");
        let dst = d.path().join("outro-disco");
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::create_dir_all(&dst).unwrap();
        fs::write(src.join("a.bin"), vec![1u8; 5000]).unwrap();
        fs::write(src.join("sub/b.bin"), vec![2u8; 300]).unwrap();
        let mut last = (0, 0);
        let m = move_with_link(&src, &dst, |c, t| last = (c, t)).unwrap();
        assert_eq!(m.bytes, 5300);
        assert_eq!(last, (5300, 5300));
        assert!(fs::symlink_metadata(&src).unwrap().file_type().is_symlink());
        assert_eq!(fs::read(src.join("sub/b.bin")).unwrap(), vec![2u8; 300], "acessível pelo link");
        assert_eq!(fs::read(dst.join("Jogos/a.bin")).unwrap().len(), 5000);
        assert!(!d.path().join("origem/Jogos.wd-movendo").exists());
        assert_eq!(m.leftover, None);
    }

    #[test]
    fn refuses_bad_targets_without_touching_anything() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("pasta");
        fs::create_dir_all(src.join("dentro")).unwrap();
        fs::write(src.join("x"), b"1").unwrap();
        assert!(move_with_link(&src, &src.join("dentro"), |_, _| {}).unwrap_err().contains("dentro da própria"));
        assert!(move_with_link(&src, &d.path().join("nao-existe"), |_, _| {}).is_err());
        fs::create_dir_all(d.path().join("dest/pasta")).unwrap();
        assert!(move_with_link(&src, &d.path().join("dest"), |_, _| {}).unwrap_err().contains("já existe"));
        assert!(src.join("x").exists());
        assert!(move_with_link(Path::new("/"), d.path(), |_, _| {}).is_err());
    }

    #[test]
    fn protects_system_folders() {
        assert!(is_protected(Path::new("C:\\Windows")));
        assert!(is_protected(Path::new("D:\\")));
        assert!(is_protected(Path::new("C:\\Program Files")));
        assert!(is_protected(Path::new("C:\\Users\\voce\\AppData\\Local")));
        assert!(!is_protected(Path::new("C:\\Program Files\\Steam")));
    }
}
