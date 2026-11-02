//! Shared safe file saving; frontends own prompts and presentation.
use crate::editor::Editor;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Default)]
pub struct SaveOptions {
    pub backup: bool,
    /// Confirm replacing a different existing destination. Never bypasses external-change checks.
    pub overwrite: bool,
}

#[derive(Debug)]
pub struct SaveOutcome {
    pub created_parent: Option<PathBuf>,
    pub backup_error: Option<std::io::Error>,
}

#[derive(Debug)]
pub enum SaveError {
    ChangedOnDisk,
    DestinationExists,
    Io(std::io::Error),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChangedOnDisk => {
                f.write_str("File changed on disk; use Save As to preserve your edits")
            }
            Self::DestinationExists => {
                f.write_str("Destination already exists; use Save As to confirm overwriting it")
            }
            Self::Io(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for SaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for SaveError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn save(
    editor: &mut Editor,
    path: &Path,
    options: SaveOptions,
) -> Result<SaveOutcome, SaveError> {
    let write_path = if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        std::fs::canonicalize(path)?
    } else {
        path.to_path_buf()
    };
    let same_target = editor
        .file_path
        .as_ref()
        .is_some_and(|original| same_file(original, path));
    match (same_target, &editor.saved_content) {
        (true, Some(saved)) if !disk_matches(&write_path, saved)? => {
            return Err(SaveError::ChangedOnDisk)
        }
        (true, Some(_)) => {}
        _ if write_path.exists() && !options.overwrite => return Err(SaveError::DestinationExists),
        _ => {}
    }
    let created_parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => {
            let missing = !parent.is_dir();
            std::fs::create_dir_all(parent)?;
            missing.then(|| parent.to_path_buf())
        }
        _ => None,
    };
    let backup_error = if options.backup && path.exists() {
        let backup = PathBuf::from(format!("{}~", path.display()));
        write_backup(path, &backup).err()
    } else {
        None
    };
    atomic_write(&write_path, &editor.rope)?;
    editor.saved_content = Some(editor.rope.clone());
    editor.file_path = Some(path.to_path_buf());
    editor.modified = false;
    let first_line = editor
        .rope
        .line(0)
        .as_str()
        .map(|s| s.trim_end_matches('\n'));
    editor.syntax_name = editor.highlighter.detect_syntax(Some(path), first_line);
    editor.highlighter.set_syntax(editor.syntax_name.as_deref());
    editor.display_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "[untitled]".into());
    Ok(SaveOutcome {
        created_parent,
        backup_error,
    })
}

fn disk_matches(path: &Path, saved: &ropey::Rope) -> std::io::Result<bool> {
    use std::io::Read;
    let mut disk = std::io::BufReader::new(std::fs::File::open(path)?);
    let mut buffer = Vec::new();
    for chunk in saved.chunks() {
        buffer.resize(chunk.len(), 0);
        match disk.read_exact(&mut buffer) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(false),
            Err(e) => return Err(e),
        }
        if buffer != chunk.as_bytes() {
            return Ok(false);
        }
    }
    Ok(disk.read(&mut [0])? == 0)
}

pub(crate) fn same_file(a: &Path, b: &Path) -> bool {
    a == b
        || match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
}

fn write_backup(path: &Path, backup_path: &Path) -> std::io::Result<()> {
    // Replacing the previous backup is the intended behaviour, and a missing
    // path is equally fine. Errors are deliberately ignored here: whatever the
    // reason the unlink failed, the `create_new` below refuses to write through
    // whatever remains, so it reports the problem without ever following it.
    let _ = std::fs::remove_file(backup_path);

    let mut src = std::fs::File::open(path)?;
    let mut dst = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(backup_path)?;

    // Same ordering rule as `atomic_write`: tighten the mode while the file is
    // still empty, so a secret file's contents are never briefly world-readable
    // under the umask default.
    #[cfg(unix)]
    {
        if let Ok(meta) = src.metadata() {
            let _ = dst.set_permissions(meta.permissions());
        }
    }

    // Streamed rather than buffered: the backup of a large file should not need
    // a second full copy of it in memory.
    std::io::copy(&mut src, &mut dst)?;
    dst.sync_all()?;
    Ok(())
}

/// Atomically write `rope` to `path` using the standard temp-file + rename
/// pattern. This protects the user's file from partial writes caused by
/// crashes, power loss, or `kill -9` mid-write.
///
/// Steps:
/// 1. Create a temp file in the same directory as `path` (so rename is on
///    the same filesystem — cross-FS rename is not atomic). The temp is
///    created with `O_EXCL` (`create_new`) so it can never follow or
///    truncate a pre-existing path (e.g. an attacker-planted symlink in a
///    shared directory).
/// 2. On Unix, when overwriting an existing file, tighten the (still empty)
///    temp file's permissions to match the target *before* writing any
///    bytes — see the in-body comment for why this ordering matters.
/// 3. Write bytes, then `sync_all()` to flush to disk before rename.
/// 4. `fs::rename(temp, path)` — POSIX rename is atomic within a filesystem.
/// 5. On any error between steps 1 and 4, best-effort remove the temp file.
fn atomic_write(path: &Path, rope: &ropey::Rope) -> std::io::Result<()> {
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };

    let stem = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "rune-save".to_string());

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let temp_name = format!(".{stem}.rune-tmp.{pid}.{nanos}");
    let temp_path = dir.join(temp_name);

    // From here on, any early return must best-effort clean up temp_path.
    let result: std::io::Result<()> = (|| {
        // `create_new(true)` => O_EXCL: fail rather than open an existing
        // path. Combined with the pid+nanos name this prevents a symlink or
        // collision at `temp_path` from redirecting/clobbering the write.
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;

        // On Unix, preserve the target file's permissions across the rename.
        // `fs::rename` replaces the target inode with the temp file's inode,
        // so without this a restrictive mode (e.g. 0o600) on the original
        // file could be weakened to the umask-default applied at temp creation.
        //
        // Crucially, apply the mode to the *empty* temp file BEFORE writing
        // the contents. The file is created at the umask default (often
        // world-readable 0o644); if we wrote the data first and tightened
        // afterwards, a copy of a secret file's contents would be briefly
        // readable by other local users. Tightening while the file is still
        // empty closes that window. The already-open write handle keeps
        // working even if we set a read-only mode like 0o400, because Unix
        // checks permissions at open() time, not on each write().
        #[cfg(unix)]
        {
            if let Ok(meta) = std::fs::metadata(path) {
                // Best-effort: don't fail the save if we can't apply perms.
                let _ = file.set_permissions(meta.permissions());
            }
        }

        {
            use std::io::Write;
            let mut writer = std::io::BufWriter::new(&mut file);
            rope.write_to(&mut writer)?;
            writer.flush()?;
        }
        file.sync_all()?;
        drop(file);

        std::fs::rename(&temp_path, path)?;
        Ok(())
    })();

    if result.is_err() {
        // Best-effort cleanup; ignore removal errors.
        let _ = std::fs::remove_file(&temp_path);
    }

    result
}
