//! Gestionnaire de processus FFmpeg : un processus par destination, afin
//! qu'une destination qui plante n'interrompe pas les autres.
//!
//! Le flux source (OBS, etc.) arrive sur un ingest RTMP local et est
//! re-publié sans ré-encodage (`-c copy`) vers chaque destination.

use std::collections::HashMap;
use std::process::Stdio;

use tokio::process::{Child, Command};

use crate::domain::StreamDestination;
use crate::{Error, Result};

pub const DEFAULT_INGEST_URL: &str = "rtmp://127.0.0.1:1935/live/omnistream";

pub struct StreamManager {
    ffmpeg_path: String,
    ingest_url: String,
    running: HashMap<String, Child>,
}

impl StreamManager {
    pub fn new(ffmpeg_path: impl Into<String>, ingest_url: impl Into<String>) -> Self {
        Self {
            ffmpeg_path: ffmpeg_path.into(),
            ingest_url: ingest_url.into(),
            running: HashMap::new(),
        }
    }

    pub fn ffmpeg_args(&self, dest: &StreamDestination) -> Vec<String> {
        [
            "-hide_banner",
            "-loglevel",
            "warning",
            "-i",
            &self.ingest_url,
            "-c",
            "copy",
            "-f",
            "flv",
            &dest.full_url(),
        ]
        .into_iter()
        .map(String::from)
        .collect()
    }

    pub fn start(&mut self, dest: &StreamDestination) -> Result<()> {
        if !dest.enabled {
            return Err(Error::Other(format!("destination désactivée: {}", dest.name)));
        }
        if self.is_running(&dest.id) {
            return Ok(());
        }
        let child = Command::new(&self.ffmpeg_path)
            .args(self.ffmpeg_args(dest))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        self.running.insert(dest.id.clone(), child);
        Ok(())
    }

    pub async fn stop(&mut self, destination_id: &str) -> Result<()> {
        let mut child = self
            .running
            .remove(destination_id)
            .ok_or_else(|| Error::UnknownDestination(destination_id.to_string()))?;
        child.kill().await?;
        Ok(())
    }

    pub async fn stop_all(&mut self) {
        for (_, mut child) in self.running.drain() {
            let _ = child.kill().await;
        }
    }

    /// Vrai tant que le processus n'a pas terminé ; les processus sortis sont purgés.
    pub fn is_running(&mut self, destination_id: &str) -> bool {
        match self.running.get_mut(destination_id) {
            Some(child) => match child.try_wait() {
                Ok(None) => true,
                _ => {
                    self.running.remove(destination_id);
                    false
                }
            },
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Platform;

    #[test]
    fn builds_copy_args_for_destination() {
        let manager = StreamManager::new("ffmpeg", DEFAULT_INGEST_URL);
        let dest = StreamDestination {
            id: "d1".into(),
            platform: Platform::Twitch,
            name: "Twitch".into(),
            rtmp_url: "rtmp://live.twitch.tv/app/".into(),
            stream_key: "KEY".into(),
            enabled: true,
        };
        let args = manager.ffmpeg_args(&dest);
        assert_eq!(args.last().unwrap(), "rtmp://live.twitch.tv/app/KEY");
        assert!(args.windows(2).any(|w| w == ["-c", "copy"]));
    }
}
