use std::path::Path;

use crate::ConfigError;

/// Refuses a secret file that group or others can read or write. Other platforms have no
/// comparable mode bits, so there this check passes everything.
pub fn check_private_file(what: &str, path: &Path) -> Result<(), ConfigError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path)
            .map_err(|source| ConfigError::Io {
                path: path.display().to_string(),
                source,
            })?
            .permissions()
            .mode()
            & 0o777;
        if mode & 0o077 != 0 {
            return Err(ConfigError::InsecureFile {
                what: what.to_owned(),
                path: path.display().to_string(),
                mode,
            });
        }
    }
    #[cfg(not(unix))]
    let _ = (what, path);
    Ok(())
}
