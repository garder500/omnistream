use crate::domain::Platform;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("aucun connecteur enregistré pour la plateforme {0}")]
    UnknownPlatform(Platform),
    #[error("{0}: pas encore implémenté")]
    NotImplemented(&'static str),
    #[error("action de modération non supportée par {platform}: {action}")]
    UnsupportedAction {
        platform: Platform,
        action: &'static str,
    },
    #[error("destination de stream inconnue: {0}")]
    UnknownDestination(String),
    #[error("erreur de base de données: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("erreur d'entrée/sortie: {0}")]
    Io(#[from] std::io::Error),
    #[error("erreur réseau: {0}")]
    Network(String),
    #[error("compte non connecté: {0}")]
    NotAuthenticated(&'static str),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Network(e.to_string())
    }
}
