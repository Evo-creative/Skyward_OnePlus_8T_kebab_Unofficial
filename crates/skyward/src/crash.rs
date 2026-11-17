//! The kernel's own record of a start that ended badly: what pstore keeps
//! in memory a reset leaves alone, copied where a later start can read it.
//! A phone whose kernel stopped comes back through its bootloader's
//! emergency download mode, and nothing on a partition is written on the
//! way, so this is the only account of why it stopped.

use anyhow::{Context, Result};
use std::ffi::CString;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// Where the kernel keeps what it wrote before a reset.
const PSTORE: &str = "/sys/fs/pstore";

/// Where a start copies it to, under /metadata, which outlives a reset and
/// is there before the data partition opens.
const INTO: &str = "/metadata/skyward/crash";

/// The files pstore keeps, by what their names begin with: the console the
/// kernel printed, its own record, its function trace, and what it left in
/// the psuedo-terminal log.
const KEPT: [&str; 4] = ["console", "dmesg", "ftrace", "pmsg"];

/// Copy what pstore holds of the start before into [`INTO`], say why this
/// start came about, and clear pstore so the next start's records are its
/// own. Gives the folder the records were kept in, where there were any.
pub fn keep() -> Result<Option<PathBuf>> {
    attach()?;
    let kept = records()?;
    if kept.is_empty() {
        return Ok(None);
    }
    let into = PathBuf::from(INTO);
    crate::settings::with_metadata(|| -> Result<()> {
        skyward_vab::metadata_writable(true)?;
        let done = (|| -> Result<()> {
            // The folder holds one start's records only: an older one
            // among them would be taken for this one's.
            if into.is_dir() {
                for entry in fs::read_dir(&into).with_context(|| format!("read {}", into.display()))? {
                    let path = entry.with_context(|| format!("read {}", into.display()))?.path();
                    let _ = fs::remove_file(path);
                }
            }
            fs::create_dir_all(&into).with_context(|| format!("make {}", into.display()))?;
            for path in &kept {
                let name = path.file_name().expect("a record has a name");
                let to = into.join(name);
                let mut file = fs::File::create(&to).with_context(|| format!("make {}", to.display()))?;
                let mut from = fs::File::open(path).with_context(|| format!("read {}", path.display()))?;
                std::io::copy(&mut from, &mut file).with_context(|| format!("write {}", to.display()))?;
                file.sync_all().with_context(|| format!("write {}", to.display()))?;
            }
            let reason = into.join("reason.txt");
            let mut file = fs::File::create(&reason).with_context(|| format!("make {}", reason.display()))?;
            writeln!(file, "boot reason: {}", skyward_sys::prop("ro.boot.bootreason").unwrap_or_default())?;
            writeln!(file, "cmdline: {}", fs::read_to_string("/proc/cmdline").unwrap_or_default().trim())?;
            file.sync_all().with_context(|| format!("write {}", reason.display()))?;
            Ok(())
        })();
        let back = skyward_vab::metadata_writable(false);
        done?;
        back
    })?;
    // Cleared, so a later start's records stand apart from the ones just
    // copied; a backend that will not clear them leaves them to be
    // overwritten in its own time.
    for path in kept {
        let _ = fs::remove_file(path);
    }
    Ok(Some(into))
}

/// Mount pstore, which a recovery's init does not, where it can be.
fn attach() -> Result<()> {
    let target = CString::new(PSTORE).expect("no NUL");
    let filesystem = CString::new("pstore").expect("no NUL");
    let source = CString::new("pstore").expect("no NUL");
    let options = CString::new("").expect("no NUL");
    let mounted = unsafe {
        libc::mount(source.as_ptr(), target.as_ptr(), filesystem.as_ptr(), 0, options.as_ptr().cast::<libc::c_void>())
    };
    if mounted == 0 {
        return Ok(());
    }
    let e = std::io::Error::last_os_error();
    // Already mounted, or nothing here keeps such a record.
    match e.raw_os_error() {
        Some(libc::EBUSY | libc::ENODEV | libc::ENOENT | libc::EPERM) => Ok(()),
        _ => Err(e).with_context(|| format!("mount pstore on {PSTORE}")),
    }
}

/// The records pstore holds now, in the order its names come.
fn records() -> Result<Vec<PathBuf>> {
    let dir = match fs::read_dir(PSTORE) {
        Ok(dir) => dir,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("read {PSTORE}")),
    };
    let mut found = Vec::new();
    for entry in dir {
        let path = entry.with_context(|| format!("read {PSTORE}"))?.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if KEPT.iter().any(|what| name.starts_with(what)) && path.is_file() {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}