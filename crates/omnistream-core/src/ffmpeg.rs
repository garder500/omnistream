//! Détection de FFmpeg (réglage, à côté de l'exécutable, puis PATH).

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde::Serialize;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct FfmpegStatus {
    pub found: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    /// Message à afficher à l'utilisateur quand FFmpeg est introuvable.
    pub message: Option<String>,
}

fn executable_name() -> &'static str {
    if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}

fn candidates(custom: Option<&str>) -> Vec<String> {
    let mut list = Vec::new();
    if let Some(p) = custom.map(str::trim).filter(|p| !p.is_empty()) {
        list.push(p.to_string());
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)) {
        for sub in ["", "ffmpeg", "bin"] {
            let p = dir.join(sub).join(executable_name());
            if p.is_file() {
                list.push(p.to_string_lossy().into_owned());
            }
        }
    }
    list.push("ffmpeg".to_string());
    list
}

pub fn command(path: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(path);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    cmd
}

async fn probe(path: &str) -> Option<String> {
    let out = tokio::time::timeout(
        Duration::from_secs(5),
        command(path).arg("-version").stdin(Stdio::null()).stderr(Stdio::null()).output(),
    )
    .await
    .ok()?
    .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).lines().next().map(str::to_string)
}

pub async fn detect(custom: Option<&str>) -> FfmpegStatus {
    for path in candidates(custom) {
        if let Some(version) = probe(&path).await {
            return FfmpegStatus { found: true, path: Some(path), version: Some(version), message: None };
        }
    }
    let how = if cfg!(windows) {
        "Installez-le avec « winget install Gyan.FFmpeg » (puis relancez Omnistream), ou téléchargez-le sur ffmpeg.org et indiquez le chemin de ffmpeg.exe dans les réglages."
    } else if cfg!(target_os = "macos") {
        "Installez-le avec « brew install ffmpeg », ou indiquez son chemin dans les réglages."
    } else {
        "Installez-le avec votre gestionnaire de paquets (ex. « sudo apt install ffmpeg »), ou indiquez son chemin dans les réglages."
    };
    FfmpegStatus {
        found: false,
        path: None,
        version: None,
        message: Some(format!(
            "FFmpeg est introuvable : il est livré avec Omnistream, l'installation semble incomplète (antivirus ?). Vous pouvez aussi l'installer vous-même. {how}"
        )),
    }
}
