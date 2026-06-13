use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

use chrono::Local;
use once_cell::sync::OnceCell;

use crate::paths;

/// Process-wide log sink. `None` once initialised but unopenable, so logging
/// degrades to a silent no-op rather than panicking or corrupting the TUI.
static LOG: OnceCell<Option<Mutex<std::fs::File>>> = OnceCell::new();

/// Open the log file for appending (see [`paths::log_file`]). Safe to call once
/// at startup; later calls are ignored.
///
/// Logs go to a file rather than stdout/stderr because the terminal is owned by
/// the raw-mode TUI — any stray print would corrupt the display.
pub fn init() {
    LOG.get_or_init(|| {
        let path = paths::log_file();

        if let Some(parent) = Path::new(&path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(file) => Some(Mutex::new(file)),
            Err(e) => {
                // The terminal may not yet be in raw mode at startup, so this one
                // diagnostic is acceptable on stderr.
                eprintln!("[logging] failed to open {}: {}; logging disabled", path, e);
                None
            }
        }
    });
}

/// Append one timestamped line to the log file. Used by the [`log_line!`] macro;
/// prefer the macro at call sites.
pub fn write_line(args: std::fmt::Arguments<'_>) {
    let Some(Some(lock)) = LOG.get() else {
        return; // Not initialised, or the file could not be opened.
    };

    if let Ok(mut file) = lock.lock() {
        let ts = Local::now().format("%Y-%m-%d %H:%M:%S");
        let _ = writeln!(file, "{} {}", ts, args);
    }
}

/// Append a timestamped line to the log file, `format!`-style.
///
/// ```ignore
/// log_line!("[pyth] stream for {} reconnecting", key);
/// ```
#[macro_export]
macro_rules! log_line {
    ($($arg:tt)*) => {
        $crate::logging::write_line(format_args!($($arg)*))
    };
}
