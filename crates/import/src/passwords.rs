//! Real import of Chrome's `Login Data` file (SQLite, under a profile
//! directory) - same real AES-256-GCM decryption as `cookies.rs`, against
//! the `logins` table's `password_value` blob.
use crate::encrypted_value;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct Password {
    pub origin_url: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug)]
pub enum ImportError {
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::Io(e) => write!(f, "failed to prepare Login Data file for import: {e}"),
            ImportError::Sqlite(e) => write!(f, "failed to read Login Data database: {e}"),
        }
    }
}

impl std::error::Error for ImportError {}

/// Same skip-on-decrypt-failure behavior as `cookies::import_cookies` - one
/// row that can't be decrypted (wrong key, `v20`) doesn't lose every other
/// real credential.
pub fn import_passwords(path: &Path, master_key: &[u8]) -> Result<Vec<Password>, ImportError> {
    let temp_path = std::env::temp_dir().join(format!("nimble-import-logins-{}.sqlite", std::process::id()));
    std::fs::copy(path, &temp_path).map_err(ImportError::Io)?;

    let result = (|| {
        let conn = rusqlite::Connection::open(&temp_path).map_err(ImportError::Sqlite)?;
        let mut stmt = conn.prepare("SELECT origin_url, username_value, password_value FROM logins").map_err(ImportError::Sqlite)?;
        let rows = stmt
            .query_map([], |row| {
                let origin_url: String = row.get(0)?;
                let username: String = row.get(1)?;
                let encrypted: Vec<u8> = row.get(2)?;
                Ok((origin_url, username, encrypted))
            })
            .map_err(ImportError::Sqlite)?;

        let mut passwords = Vec::new();
        for row in rows {
            let (origin_url, username, encrypted) = row.map_err(ImportError::Sqlite)?;
            if let Ok(password) = encrypted_value::decrypt(master_key, &encrypted) {
                passwords.push(Password { origin_url, username, password });
            }
        }
        Ok(passwords)
    })();

    let _ = std::fs::remove_file(&temp_path);
    result
}
