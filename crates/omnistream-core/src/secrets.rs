//! Stockage des secrets (clés de stream) dans le trousseau de l'OS :
//! Credential Manager sous Windows, Keychain sous macOS, keyutils sous Linux.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::{Error, Result};

const SERVICE: &str = "omnistream";

pub trait SecretStore: Send + Sync {
    fn set(&self, name: &str, value: &str) -> Result<()>;
    fn get(&self, name: &str) -> Result<Option<String>>;
    fn delete(&self, name: &str) -> Result<()>;
}

pub struct KeyringStore;

fn keyring_err(e: keyring::Error) -> Error {
    Error::Other(format!("trousseau de l'OS indisponible: {e}"))
}

impl SecretStore for KeyringStore {
    fn set(&self, name: &str, value: &str) -> Result<()> {
        keyring::Entry::new(SERVICE, name)
            .map_err(keyring_err)?
            .set_password(value)
            .map_err(keyring_err)
    }

    fn get(&self, name: &str) -> Result<Option<String>> {
        match keyring::Entry::new(SERVICE, name)
            .map_err(keyring_err)?
            .get_password()
        {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(keyring_err(e)),
        }
    }

    fn delete(&self, name: &str) -> Result<()> {
        match keyring::Entry::new(SERVICE, name)
            .map_err(keyring_err)?
            .delete_credential()
        {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(keyring_err(e)),
        }
    }
}

/// Stockage en mémoire (tests).
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn set(&self, name: &str, value: &str) -> Result<()> {
        self.0.lock().unwrap().insert(name.into(), value.into());
        Ok(())
    }

    fn get(&self, name: &str) -> Result<Option<String>> {
        Ok(self.0.lock().unwrap().get(name).cloned())
    }

    fn delete(&self, name: &str) -> Result<()> {
        self.0.lock().unwrap().remove(name);
        Ok(())
    }
}
