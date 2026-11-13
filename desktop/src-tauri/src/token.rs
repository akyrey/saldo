//! The session token in the operating system's keychain, never in files or
//! the webview's storage. Each function is a `#[tauri::command]`: the web app
//! calls it with `invoke("load_token")`, like a controller action or an RPC
//! handler in Go. Errors travel to JavaScript as plain text.

use keyring::Entry;

/// The keychain entry: service and account name.
const SERVICE: &str = "app.saldo.desktop";
const ACCOUNT: &str = "session";

#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    #[error("the keychain is not available: {0}")]
    Keychain(#[from] keyring::Error),
}

// Tauri sends a command's error to JavaScript as JSON; a message is enough.
impl serde::Serialize for TokenError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

fn entry() -> Result<Entry, TokenError> {
    Ok(Entry::new(SERVICE, ACCOUNT)?)
}

/// The saved token, or `None` before the first login and after logout.
#[tauri::command]
pub fn load_token() -> Result<Option<String>, TokenError> {
    match entry()?.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(other) => Err(other.into()),
    }
}

#[tauri::command]
pub fn save_token(token: String) -> Result<(), TokenError> {
    Ok(entry()?.set_password(&token)?)
}

/// Forgets the token; forgetting a token that is not there is not an error.
#[tauri::command]
pub fn delete_token() -> Result<(), TokenError> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(other) => Err(other.into()),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn saves_loads_and_forgets_the_token() {
        // An in-memory keychain, so tests never touch the real one. The mock
        // keeps nothing between entries, so one `Entry` is used throughout.
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let entry = entry().unwrap();
        assert!(matches!(entry.get_password(), Err(keyring::Error::NoEntry)));
        entry.set_password("secret").unwrap();
        assert_eq!(entry.get_password().unwrap(), "secret");
        entry.delete_credential().unwrap();
        assert!(matches!(entry.get_password(), Err(keyring::Error::NoEntry)));
    }

    #[test]
    fn errors_reach_javascript_as_text() {
        let error = TokenError::Keychain(keyring::Error::NoEntry);
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.starts_with("\"the keychain is not available"));
    }
}
