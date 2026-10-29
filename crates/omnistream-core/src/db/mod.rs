//! Base SQLite locale : comptes, destinations de stream, préférences, historique du chat.

mod automation;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

use crate::auth::TokenSet;
use crate::domain::{Account, Channel, Platform, StreamDestination};
use crate::Result;

// Ordre = version (`PRAGMA user_version`) : n'insérer qu'en fin de liste, jamais au milieu.
// Le numéro du fichier (0130...) sert à repérer les migrations des différentes fonctionnalités.
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/0001_init.sql"),
    include_str!("migrations/0002_tokens.sql"),
    include_str!("migrations/0130_automations.sql"),
];

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_connection(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        let db = Self { conn: Mutex::new(conn) };
        db.migrate()?;
        Ok(db)
    }

    /// Applique les migrations manquantes, suivies via `PRAGMA user_version`.
    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let current: usize = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
            conn.execute_batch(sql)?;
            conn.pragma_update(None, "user_version", i + 1)?;
        }
        Ok(())
    }

    pub fn save_account(&self, account: &Account) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO accounts (id, platform, display_name, login) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET display_name = excluded.display_name, login = excluded.login",
            (&account.id, account.platform.as_str(), &account.display_name, &account.login),
        )?;
        Ok(())
    }

    pub fn list_accounts(&self) -> Result<Vec<Account>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, platform, display_name, login FROM accounts ORDER BY display_name")?;
        let rows = stmt.query_map([], |r| {
            Ok(Account {
                id: r.get(0)?,
                platform: Platform::from(r.get::<_, String>(1)?.as_str()),
                display_name: r.get(2)?,
                login: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Supprime le compte, ses jetons (cascade) et détache ses chaînes.
    pub fn delete_account(&self, id: &str) -> Result<()> {
        self.conn.lock().unwrap().execute("DELETE FROM accounts WHERE id = ?1", [id])?;
        Ok(())
    }

    // Les jetons sont stockés en clair dans la base locale ; à migrer vers le
    // trousseau de l'OS (Credential Manager sous Windows).
    pub fn save_tokens(&self, account_id: &str, t: &TokenSet) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO account_tokens (account_id, access_token, refresh_token, expires_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(account_id) DO UPDATE SET access_token = excluded.access_token,
               refresh_token = excluded.refresh_token, expires_at = excluded.expires_at",
            (account_id, &t.access_token, &t.refresh_token, t.expires_at),
        )?;
        Ok(())
    }

    pub fn load_tokens(&self, account_id: &str) -> Result<Option<TokenSet>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT access_token, refresh_token, expires_at FROM account_tokens WHERE account_id = ?1",
        )?;
        let mut rows = stmt.query_map([account_id], |r| {
            Ok(TokenSet { access_token: r.get(0)?, refresh_token: r.get(1)?, expires_at: r.get(2)? })
        })?;
        Ok(rows.next().transpose()?)
    }

    pub fn save_channel(&self, c: &Channel) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO channels (id, platform, external_id, display_name) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET display_name = excluded.display_name",
            (&c.id, c.platform.as_str(), &c.external_id, &c.display_name),
        )?;
        Ok(())
    }

    pub fn list_channels(&self) -> Result<Vec<Channel>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, platform, external_id, display_name FROM channels ORDER BY rowid")?;
        let rows = stmt.query_map([], |r| {
            Ok(Channel {
                id: r.get(0)?,
                platform: Platform::from(r.get::<_, String>(1)?.as_str()),
                external_id: r.get(2)?,
                display_name: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn delete_channel(&self, id: &str) -> Result<()> {
        self.conn.lock().unwrap().execute("DELETE FROM channels WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query_map([key], |r| r.get(0))?;
        Ok(rows.next().transpose()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            (key, value),
        )?;
        Ok(())
    }

    pub fn save_destination(&self, d: &StreamDestination) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO stream_destinations (id, platform, name, rtmp_url, stream_key, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, rtmp_url = excluded.rtmp_url,
               stream_key = excluded.stream_key, enabled = excluded.enabled",
            (&d.id, d.platform.as_str(), &d.name, &d.rtmp_url, &d.stream_key, d.enabled),
        )?;
        Ok(())
    }

    pub fn get_destination(&self, id: &str) -> Result<Option<StreamDestination>> {
        Ok(self.list_destinations()?.into_iter().find(|d| d.id == id))
    }

    pub fn delete_destination(&self, id: &str) -> Result<()> {
        self.conn.lock().unwrap().execute("DELETE FROM stream_destinations WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn list_destinations(&self) -> Result<Vec<StreamDestination>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, platform, name, rtmp_url, stream_key, enabled FROM stream_destinations ORDER BY rowid",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(StreamDestination {
                id: r.get(0)?,
                platform: Platform::from(r.get::<_, String>(1)?.as_str()),
                name: r.get(2)?,
                rtmp_url: r.get(3)?,
                stream_key: r.get(4)?,
                enabled: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_destinations_and_accounts() {
        let db = Database::open_in_memory().unwrap();
        db.save_account(&Account {
            id: "a1".into(),
            platform: Platform::Kick,
            display_name: "Moi".into(),
            login: "moi".into(),
        })
        .unwrap();
        assert_eq!(db.list_accounts().unwrap().len(), 1);

        let dest = StreamDestination {
            id: "d1".into(),
            platform: Platform::Youtube,
            name: "YT".into(),
            rtmp_url: "rtmp://a.rtmp.youtube.com/live2".into(),
            stream_key: "k".into(),
            enabled: true,
        };
        db.save_destination(&dest).unwrap();
        assert_eq!(db.list_destinations().unwrap(), vec![dest]);
    }

    #[test]
    fn tokens_channels_and_settings() {
        let db = Database::open_in_memory().unwrap();
        db.save_account(&Account {
            id: "42".into(),
            platform: Platform::Twitch,
            display_name: "Garder".into(),
            login: "garder".into(),
        })
        .unwrap();
        let tokens = TokenSet { access_token: "a".into(), refresh_token: Some("r".into()), expires_at: Some(10) };
        db.save_tokens("42", &tokens).unwrap();
        assert_eq!(db.load_tokens("42").unwrap().unwrap().refresh_token.as_deref(), Some("r"));
        db.delete_account("42").unwrap();
        assert!(db.load_tokens("42").unwrap().is_none());

        let ch = Channel { id: "twitch:garder".into(), platform: Platform::Twitch, external_id: "garder".into(), display_name: "garder".into() };
        db.save_channel(&ch).unwrap();
        assert_eq!(db.list_channels().unwrap(), vec![ch]);
        db.delete_channel("twitch:garder").unwrap();
        assert!(db.list_channels().unwrap().is_empty());

        assert_eq!(db.get_setting("k").unwrap(), None);
        db.set_setting("k", "v").unwrap();
        db.set_setting("k", "w").unwrap();
        assert_eq!(db.get_setting("k").unwrap().as_deref(), Some("w"));
    }
}
