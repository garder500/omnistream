//! Historique des sessions de live : sessions, échantillons de spectateurs et
//! événements normalisés, plus l'export CSV.

use std::collections::BTreeMap;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::Database;
use crate::domain::{Platform, StreamEvent};
use crate::Result;

/// Silence (aucun live vu) au-delà duquel une session est considérée terminée.
/// Large devant le sondage de 30 s pour absorber un échec réseau ponctuel.
pub const SESSION_GAP_MS: i64 = 150_000;

/// Deux sondages plus proches que cela sont fusionnés (l'interface peut relancer le sondage
/// à chaque changement de comptes ou de chaînes, en plus du minuteur de 30 s).
pub const MIN_POLL_INTERVAL_MS: i64 = 10_000;

/// Nombre de spectateurs d'une chaîne en direct à un instant donné.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerSampleInput {
    pub channel_id: String,
    pub platform: Platform,
    pub viewers: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub started_at_ms: i64,
    /// `None` tant que la session est en cours.
    pub ended_at_ms: Option<i64>,
    pub peak_viewers: u32,
    pub avg_viewers: u32,
    pub event_count: u32,
    /// Nombre d'événements par type (`follow`, `sub`, `gift`, `raid`, `donation`, `bits`).
    pub events_by_kind: BTreeMap<String, u32>,
}

/// Total de spectateurs à un sondage, avec sa ventilation par plateforme.
/// C'est une somme brute : un même spectateur présent sur deux plateformes compte deux fois.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerPoint {
    pub ts_ms: i64,
    pub total: u32,
    pub by_platform: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionDetail {
    pub summary: SessionSummary,
    pub points: Vec<ViewerPoint>,
    pub events: Vec<StreamEvent>,
}

impl Database {
    /// Enregistre un sondage des lives. Ouvre une session au premier live vu, la
    /// ferme (à l'instant du dernier live vu) après [`SESSION_GAP_MS`] sans live.
    pub fn record_poll(&self, now_ms: i64, samples: &[ViewerSampleInput]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let open: Option<(String, i64)> = tx
            .query_row("SELECT id, last_live_ms FROM sessions WHERE ended_at_ms IS NULL ORDER BY started_at_ms DESC LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?;

        if samples.is_empty() {
            if let Some((id, last_live)) = open {
                if now_ms - last_live >= SESSION_GAP_MS {
                    tx.execute("UPDATE sessions SET ended_at_ms = ?2 WHERE id = ?1", params![id, last_live])?;
                }
            }
            return Ok(tx.commit()?);
        }

        if matches!(&open, Some((_, last_live)) if (0..MIN_POLL_INTERVAL_MS).contains(&(now_ms - last_live))) {
            return Ok(tx.commit()?);
        }
        let total: i64 = samples.iter().map(|s| i64::from(s.viewers)).sum();
        // Une session restée ouverte trop longtemps sans live (appli fermée) est d'abord close.
        let id = match open {
            Some((id, last_live)) if now_ms - last_live < SESSION_GAP_MS => id,
            stale => {
                if let Some((id, last_live)) = stale {
                    tx.execute("UPDATE sessions SET ended_at_ms = ?2 WHERE id = ?1", params![id, last_live])?;
                }
                let id = format!("s{now_ms}");
                tx.execute(
                    "INSERT INTO sessions (id, started_at_ms, last_live_ms, peak_viewers) VALUES (?1, ?2, ?2, 0)",
                    params![id, now_ms],
                )?;
                id
            }
        };
        tx.execute(
            "UPDATE sessions SET last_live_ms = ?2, peak_viewers = MAX(peak_viewers, ?3) WHERE id = ?1",
            params![id, now_ms, total],
        )?;
        for s in samples {
            tx.execute(
                "INSERT INTO viewer_samples (session_id, ts_ms, channel_id, platform, viewers) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, now_ms, s.channel_id, s.platform.as_str(), s.viewers],
            )?;
        }
        Ok(tx.commit()?)
    }

    /// Enregistre un événement, rattaché à la session en cours (ou tout juste terminée) s'il y en a une.
    /// Un événement déjà vu (même id sur la même chaîne) est ignoré.
    pub fn record_event(&self, event: &StreamEvent, now_ms: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        // Session encore vivante, ou close depuis moins d'un délai de grâce (raid tardif).
        // Une session restée ouverte par une appli fermée n'adopte pas les événements d'après.
        let session: Option<String> = conn
            .query_row(
                "SELECT id FROM sessions WHERE last_live_ms > ?1 - ?2 ORDER BY started_at_ms DESC LIMIT 1",
                params![now_ms, SESSION_GAP_MS],
                |r| r.get(0),
            )
            .optional()?;
        let ts = if event.timestamp_ms > 0 { event.timestamp_ms } else { now_ms };
        conn.execute(
            "INSERT OR IGNORE INTO session_events
               (session_id, event_id, channel_id, platform, kind, who, amount, message, payload, timestamp_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                session,
                event.id,
                event.channel_id,
                event.platform.as_str(),
                event.kind.name(),
                event.who,
                event.kind.amount() as i64,
                event.message,
                serde_json::to_string(event).map_err(|e| crate::Error::Other(e.to_string()))?,
                ts,
            ],
        )?;
        Ok(())
    }

    /// Sessions, de la plus récente à la plus ancienne.
    pub fn list_sessions(&self, limit: u32) -> Result<Vec<SessionSummary>> {
        load_summaries(&self.conn.lock().unwrap(), None, limit)
    }

    pub fn session_detail(&self, id: &str) -> Result<Option<SessionDetail>> {
        let conn = self.conn.lock().unwrap();
        let Some(summary) = load_summaries(&conn, Some(id), 1)?.pop() else {
            return Ok(None);
        };

        let mut points: BTreeMap<i64, ViewerPoint> = BTreeMap::new();
        let mut stmt = conn.prepare("SELECT ts_ms, platform, viewers FROM viewer_samples WHERE session_id = ?1 ORDER BY ts_ms")?;
        for row in stmt.query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, u32>(2)?)))? {
            let (ts_ms, platform, viewers) = row?;
            let p = points.entry(ts_ms).or_insert_with(|| ViewerPoint { ts_ms, total: 0, by_platform: BTreeMap::new() });
            p.total += viewers;
            *p.by_platform.entry(platform).or_insert(0) += viewers;
        }

        let mut stmt = conn.prepare("SELECT payload FROM session_events WHERE session_id = ?1 ORDER BY timestamp_ms, seq")?;
        let events = stmt
            .query_map([id], |r| r.get::<_, String>(0))?
            .filter_map(|p| p.ok().and_then(|p| serde_json::from_str::<StreamEvent>(&p).ok()))
            .collect();
        Ok(Some(SessionDetail { summary, points: points.into_values().collect(), events }))
    }

    pub fn delete_session(&self, id: &str) -> Result<()> {
        self.conn.lock().unwrap().execute("DELETE FROM sessions WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Export CSV d'une session : une ligne par échantillon de spectateurs
    /// (`type=viewers`) et par événement (`type=event`), triées par date.
    pub fn export_session_csv(&self, id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?1)", [id], |r| r.get(0))?;
        if !exists {
            return Ok(None);
        }
        let mut rows: Vec<(i64, [String; 10])> = Vec::new();
        let mut stmt = conn.prepare("SELECT ts_ms, platform, channel_id, viewers FROM viewer_samples WHERE session_id = ?1")?;
        for r in stmt.query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?)))? {
            let (ts, platform, channel, viewers) = r?;
            rows.push((
                ts,
                ["viewers".into(), iso_utc(ts), ts.to_string(), platform, channel, viewers.to_string(), String::new(), String::new(), String::new(), String::new()],
            ));
        }
        let mut stmt = conn.prepare(
            "SELECT timestamp_ms, platform, channel_id, kind, who, amount, COALESCE(message, '') FROM session_events WHERE session_id = ?1",
        )?;
        for r in stmt.query_map([id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, String>(6)?,
            ))
        })? {
            let (ts, platform, channel, kind, who, amount, message) = r?;
            rows.push((
                ts,
                ["event".into(), iso_utc(ts), ts.to_string(), platform, channel, String::new(), kind, who, amount.to_string(), message],
            ));
        }
        rows.sort_by_key(|(ts, _)| *ts);

        let mut out = String::from("type,date_utc,timestamp_ms,platform,channel,viewers,event,who,amount,message\n");
        for (_, r) in rows {
            out.push_str(&r.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(","));
            out.push('\n');
        }
        Ok(Some(out))
    }
}

/// Résumés de sessions ; `id` restreint à une seule session.
fn load_summaries(conn: &Connection, id: Option<&str>, limit: u32) -> Result<Vec<SessionSummary>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.started_at_ms, s.ended_at_ms, s.peak_viewers,
                COALESCE((SELECT CAST(ROUND(AVG(t)) AS INTEGER)
                          FROM (SELECT SUM(viewers) AS t FROM viewer_samples WHERE session_id = s.id GROUP BY ts_ms)), 0)
         FROM sessions s WHERE (?2 IS NULL OR s.id = ?2) ORDER BY s.started_at_ms DESC LIMIT ?1",
    )?;
    let mut out = stmt
        .query_map(params![limit, id], |r| {
            Ok(SessionSummary {
                id: r.get(0)?,
                started_at_ms: r.get(1)?,
                ended_at_ms: r.get(2)?,
                peak_viewers: r.get(3)?,
                avg_viewers: r.get(4)?,
                event_count: 0,
                events_by_kind: BTreeMap::new(),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut counts = conn.prepare("SELECT kind, COUNT(*) FROM session_events WHERE session_id = ?1 GROUP BY kind")?;
    for s in &mut out {
        for row in counts.query_map([&s.id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)))? {
            let (kind, n) = row?;
            s.event_count += n;
            s.events_by_kind.insert(kind, n);
        }
    }
    Ok(out)
}

/// Échappe un champ CSV (RFC 4180) et neutralise l'injection de formules des tableurs.
fn csv_field(value: &str) -> String {
    let mut v = value.to_string();
    if v.trim_start().starts_with(['=', '+', '-', '@', '\t', '\r']) && v.parse::<f64>().is_err() {
        v.insert(0, '\'');
    }
    if v.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", v.replace('"', "\"\""))
    } else {
        v
    }
}

/// Millisecondes Unix vers `AAAA-MM-JJThh:mm:ssZ` (algorithme « civil from days » de H. Hinnant).
fn iso_utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::StreamEventKind;

    fn sample(ch: &str, p: Platform, v: u32) -> ViewerSampleInput {
        ViewerSampleInput { channel_id: ch.into(), platform: p, viewers: v }
    }

    fn event(id: &str, kind: StreamEventKind, who: &str, ts: i64) -> StreamEvent {
        StreamEvent {
            id: id.into(),
            channel_id: "twitch:garder".into(),
            platform: Platform::Twitch,
            who: who.into(),
            kind,
            message: None,
            timestamp_ms: ts,
        }
    }

    #[test]
    fn iso_dates() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_utc(1_709_164_800_000 + 1000), "2024-02-29T00:00:01Z");
    }

    #[test]
    fn session_opens_samples_and_closes_after_gap() {
        let db = Database::open_in_memory().unwrap();
        db.record_poll(1_000, &[sample("a", Platform::Twitch, 10), sample("b", Platform::Kick, 5)]).unwrap();
        db.record_poll(31_000, &[sample("a", Platform::Twitch, 30), sample("b", Platform::Kick, 5)]).unwrap();
        // Un sondage vide dans le délai de grâce ne ferme pas la session.
        db.record_poll(61_000, &[]).unwrap();
        assert_eq!(db.list_sessions(10).unwrap()[0].ended_at_ms, None);
        db.record_poll(31_000 + SESSION_GAP_MS, &[]).unwrap();

        let sessions = db.list_sessions(10).unwrap();
        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        assert_eq!((s.peak_viewers, s.avg_viewers, s.ended_at_ms), (35, 25, Some(31_000)));

        let d = db.session_detail(&s.id).unwrap().unwrap();
        assert_eq!(d.points.len(), 2);
        assert_eq!(d.points[1].total, 35);
        assert_eq!(d.points[1].by_platform["kick"], 5);

        // Un nouveau live après la fermeture ouvre une autre session.
        db.record_poll(1_000_000, &[sample("a", Platform::Twitch, 1)]).unwrap();
        assert_eq!(db.list_sessions(10).unwrap().len(), 2);
    }

    #[test]
    fn close_polls_are_merged() {
        let db = Database::open_in_memory().unwrap();
        db.record_poll(1_000, &[sample("a", Platform::Twitch, 10)]).unwrap();
        db.record_poll(2_000, &[sample("a", Platform::Twitch, 99)]).unwrap();
        let id = db.list_sessions(1).unwrap()[0].id.clone();
        assert_eq!(db.session_detail(&id).unwrap().unwrap().points.len(), 1);
    }

    #[test]
    fn stale_open_session_is_closed_when_a_new_live_appears() {
        let db = Database::open_in_memory().unwrap();
        db.record_poll(1_000, &[sample("a", Platform::Twitch, 10)]).unwrap();
        db.record_poll(10_000_000, &[sample("a", Platform::Twitch, 4)]).unwrap();
        let sessions = db.list_sessions(10).unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[1].ended_at_ms, Some(1_000));
        assert_eq!(sessions[0].ended_at_ms, None);
    }

    #[test]
    fn events_are_attached_deduped_and_exported() {
        let db = Database::open_in_memory().unwrap();
        db.save_channel(&crate::domain::Channel {
            id: "twitch:garder".into(),
            platform: Platform::Twitch,
            external_id: "garder".into(),
            display_name: "garder".into(),
        })
        .unwrap();
        // Hors session : conservé mais rattaché à aucune session.
        db.record_event(&event("e0", StreamEventKind::Follow, "Avant", 500), 500).unwrap();
        db.record_poll(1_000, &[sample("twitch:garder", Platform::Twitch, 12)]).unwrap();
        let raid = event("e1", StreamEventKind::Raid { viewers: 85 }, "=cmd(\"x\")", 2_000);
        db.record_event(&raid, 2_000).unwrap();
        db.record_event(&raid, 2_000).unwrap();
        db.record_event(&event("", StreamEventKind::Bits { amount: 100 }, "Fan, Jr", 3_000), 3_000).unwrap();
        db.record_event(&event("", StreamEventKind::Donation { amount_cents: 500, currency: "EUR".into() }, "Don", 0), 4_000).unwrap();

        let id = db.list_sessions(1).unwrap()[0].id.clone();
        let d = db.session_detail(&id).unwrap().unwrap();
        assert_eq!(d.events.len(), 3);
        assert_eq!(d.summary.event_count, 3);
        assert_eq!(d.summary.events_by_kind["raid"], 1);
        assert_eq!(d.events[0], raid);

        let csv = db.export_session_csv(&id).unwrap().unwrap();
        let lines: Vec<_> = csv.lines().collect();
        assert_eq!(lines.len(), 5);
        assert!(lines[0].starts_with("type,date_utc"));
        assert!(lines[1].starts_with("viewers,1970-01-01T00:00:01Z,1000,twitch,twitch:garder,12"));
        // Formule neutralisée, virgule échappée.
        assert!(lines[2].contains("\"'=cmd(\"\"x\"\")\""), "{}", lines[2]);
        assert!(lines[3].contains("\"Fan, Jr\",100"), "{}", lines[3]);
        assert!(lines[4].ends_with(",donation,Don,500,"), "{}", lines[4]);
        assert!(db.export_session_csv("nope").unwrap().is_none());
        assert_eq!(csv_field(" =1+1"), "' =1+1");

        // Un raid tardif (après fermeture, dans le délai de grâce) reste dans la session ;
        // un événement bien après n'est rattaché à rien.
        db.record_event(&event("late", StreamEventKind::Follow, "Tard", 0), 1_000 + SESSION_GAP_MS - 1).unwrap();
        db.record_event(&event("later", StreamEventKind::Follow, "Très tard", 0), 99_000_000).unwrap();
        assert_eq!(db.session_detail(&id).unwrap().unwrap().events.len(), 4);
    }
}
