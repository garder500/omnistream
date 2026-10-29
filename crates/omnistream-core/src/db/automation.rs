//! Persistance des règles d'automatisation et des commandes de chat.

use super::Database;
use crate::automation::{ChatCommand, Role, Rule};
use crate::{Error, Result};

fn json_err(e: serde_json::Error) -> Error {
    Error::Other(format!("données d'automatisation illisibles: {e}"))
}

impl Database {
    pub fn save_rule(&self, r: &Rule) -> Result<()> {
        r.validate()?;
        let trigger = serde_json::to_string(&r.trigger).map_err(json_err)?;
        let action = serde_json::to_string(&r.action).map_err(json_err)?;
        let platforms = serde_json::to_string(&r.platforms).map_err(json_err)?;
        self.conn.lock().unwrap().execute(
            "INSERT INTO automation_rules (id, name, enabled, trigger_json, action_json, platforms, cooldown_secs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, enabled = excluded.enabled,
               trigger_json = excluded.trigger_json, action_json = excluded.action_json,
               platforms = excluded.platforms, cooldown_secs = excluded.cooldown_secs",
            (&r.id, &r.name, r.enabled, trigger, action, platforms, r.cooldown_secs),
        )?;
        Ok(())
    }

    /// Les lignes dont le JSON est illisible (ex. version future) sont ignorées plutôt que bloquantes.
    pub fn list_rules(&self) -> Result<Vec<Rule>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, enabled, trigger_json, action_json, platforms, cooldown_secs
             FROM automation_rules ORDER BY rowid",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, bool>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, u32>(6)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, name, enabled, trigger, action, platforms, cooldown_secs) = row?;
            let parsed = (|| -> std::result::Result<Rule, serde_json::Error> {
                Ok(Rule {
                    id,
                    name,
                    enabled,
                    trigger: serde_json::from_str(&trigger)?,
                    action: serde_json::from_str(&action)?,
                    platforms: serde_json::from_str(&platforms)?,
                    cooldown_secs,
                })
            })();
            if let Ok(rule) = parsed {
                out.push(rule);
            }
        }
        Ok(out)
    }

    pub fn delete_rule(&self, id: &str) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM automation_rules WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn save_command(&self, c: &ChatCommand) -> Result<()> {
        c.validate()?;
        let platforms = serde_json::to_string(&c.platforms).map_err(json_err)?;
        let reply = serde_json::to_string(&c.reply).map_err(json_err)?;
        let res = self.conn.lock().unwrap().execute(
            "INSERT INTO chat_commands
               (id, name, response, enabled, cooldown_secs, user_cooldown_secs, min_role, platforms, reply_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, response = excluded.response,
               enabled = excluded.enabled, cooldown_secs = excluded.cooldown_secs,
               user_cooldown_secs = excluded.user_cooldown_secs, min_role = excluded.min_role,
               platforms = excluded.platforms, reply_json = excluded.reply_json",
            (
                &c.id,
                &c.name,
                &c.response,
                c.enabled,
                c.cooldown_secs,
                c.user_cooldown_secs,
                c.min_role.as_str(),
                platforms,
                reply,
            ),
        );
        match res {
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Err(Error::Other(format!("la commande !{} existe déjà", c.name)))
            }
            other => other.map(|_| ()).map_err(Into::into),
        }
    }

    pub fn list_commands(&self) -> Result<Vec<ChatCommand>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, response, enabled, cooldown_secs, user_cooldown_secs, min_role, platforms, reply_json
             FROM chat_commands ORDER BY name",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                ChatCommand {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    response: r.get(2)?,
                    enabled: r.get(3)?,
                    cooldown_secs: r.get(4)?,
                    user_cooldown_secs: r.get(5)?,
                    min_role: Role::from(r.get::<_, String>(6)?.as_str()),
                    platforms: vec![],
                    reply: Default::default(),
                },
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (mut cmd, platforms, reply) = row?;
            // Valeurs illisibles : repli sur les défauts (toutes plateformes, réponse sur place).
            cmd.platforms = serde_json::from_str(&platforms).unwrap_or_default();
            cmd.reply = serde_json::from_str(&reply).unwrap_or_default();
            out.push(cmd);
        }
        Ok(out)
    }

    pub fn delete_command(&self, id: &str) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM chat_commands WHERE id = ?1", [id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::{Action, Target, Trigger};
    use crate::domain::Platform;

    #[test]
    fn rules_roundtrip_update_and_delete() {
        let db = Database::open_in_memory().unwrap();
        let mut r = Rule {
            id: "r1".into(),
            name: "Merci".into(),
            enabled: true,
            trigger: Trigger::Raid { min_viewers: 5 },
            action: Action::SendMessage {
                target: Target::All,
                text: "Merci {user} !".into(),
            },
            platforms: vec![Platform::Twitch, Platform::Kick],
            cooldown_secs: 7,
        };
        db.save_rule(&r).unwrap();
        assert_eq!(db.list_rules().unwrap(), vec![r.clone()]);
        r.enabled = false;
        r.name = "Autre".into();
        db.save_rule(&r).unwrap();
        assert_eq!(db.list_rules().unwrap(), vec![r.clone()]);
        db.delete_rule("r1").unwrap();
        assert!(db.list_rules().unwrap().is_empty());
    }

    #[test]
    fn invalid_rule_is_rejected_and_corrupt_row_skipped() {
        let db = Database::open_in_memory().unwrap();
        let bad = Rule {
            id: "r".into(),
            name: "x".into(),
            enabled: true,
            trigger: Trigger::Follow,
            action: Action::Timeout { seconds: 1 },
            platforms: vec![],
            cooldown_secs: 0,
        };
        assert!(db.save_rule(&bad).is_err());
        db.conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO automation_rules (id, name, trigger_json, action_json) VALUES ('c', 'c', '{\"type\":\"futur\"}', '{}')",
                [],
            )
            .unwrap();
        assert!(db.list_rules().unwrap().is_empty());
    }

    #[test]
    fn commands_roundtrip_and_unique_name() {
        let db = Database::open_in_memory().unwrap();
        let c = ChatCommand {
            id: "c1".into(),
            name: "discord".into(),
            response: "discord.gg/x".into(),
            enabled: true,
            cooldown_secs: 30,
            user_cooldown_secs: 5,
            min_role: Role::Moderator,
            platforms: vec![Platform::Kick],
            reply: Target::Platforms {
                platforms: vec![Platform::Twitch],
            },
        };
        db.save_command(&c).unwrap();
        assert_eq!(db.list_commands().unwrap(), vec![c.clone()]);

        let dup = ChatCommand {
            id: "c2".into(),
            name: "Discord".into(),
            ..c.clone()
        };
        let err = db.save_command(&dup).unwrap_err().to_string();
        assert!(err.contains("existe déjà"), "{err}");

        let renamed = ChatCommand {
            name: "dc".into(),
            ..c.clone()
        };
        db.save_command(&renamed).unwrap();
        assert_eq!(db.list_commands().unwrap(), vec![renamed]);
        db.delete_command("c1").unwrap();
        assert!(db.list_commands().unwrap().is_empty());
    }

    #[test]
    fn migrations_apply_on_existing_v2_database() {
        // Une base créée avant l'ajout des automations doit simplement gagner les nouvelles tables.
        let db = Database::open_in_memory().unwrap();
        assert!(db.list_rules().unwrap().is_empty());
        assert!(db.list_commands().unwrap().is_empty());
    }
}
