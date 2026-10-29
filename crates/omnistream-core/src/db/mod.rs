//! Base SQLite locale : comptes, destinations de stream, préférences, historique du chat.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

use crate::domain::{Account, Platform, StreamDestination};
use crate::Result;

const MIGRATIONS: &[&str] = &[include_str!("migrations/0001_init.sql")];

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
            "INSERT INTO accounts (id, platform, display_name) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET display_name = excluded.display_name",
            (&account.id, account.platform.as_str(), &account.display_name),
        )?;
        Ok(())
    }

    pub fn list_accounts(&self) -> Result<Vec<Account>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, platform, display_name FROM accounts ORDER BY display_name")?;
        let rows = stmt.query_map([], |r| {
            Ok(Account {
                id: r.get(0)?,
                platform: Platform::from(r.get::<_, String>(1)?.as_str()),
                display_name: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
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

    pub fn list_destinations(&self) -> Result<Vec<StreamDestination>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, platform, name, rtmp_url, stream_key, enabled FROM stream_destinations ORDER BY name",
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
}
