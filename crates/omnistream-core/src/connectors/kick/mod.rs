//! Connecteur Kick via l'API publique officielle : envoi de messages, modération,
//! infos de live et titre/catégorie. Le chat en lecture n'est pas disponible : l'API
//! officielle ne le diffuse que par webhooks (URL publique), inadaptés à une appli de bureau.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use reqwest::Method;
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex};

use super::{ChatConnector, ChatEvent, TokenHook};
use crate::auth::kick::API_URL;
use crate::auth::{self, now_secs, TokenSet};
use crate::domain::{
    Category, Channel, ModerationAction, ModerationKind, Platform, StreamInfo, StreamInfoUpdate,
};
use crate::{Error, Result};

/// Limite de longueur d'un message de chat Kick.
const MAX_MESSAGE_CHARS: usize = 500;

#[derive(Clone)]
pub struct KickApp {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Clone)]
pub struct KickAccount {
    pub user_id: String,
    pub tokens: TokenSet,
}

#[derive(Default)]
struct Config {
    app: Option<KickApp>,
    account: Option<KickAccount>,
}

struct Inner {
    http: reqwest::Client,
    config: Mutex<Config>,
    /// slug -> identifiant numérique du diffuseur.
    broadcasters: StdMutex<HashMap<String, u64>>,
    hook: StdMutex<Option<TokenHook>>,
}

#[derive(Clone)]
pub struct KickConnector {
    inner: Arc<Inner>,
}

impl Default for KickConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl KickConnector {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                http: reqwest::Client::new(),
                config: Mutex::new(Config::default()),
                broadcasters: StdMutex::default(),
                hook: StdMutex::default(),
            }),
        }
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.inner.http
    }

    pub async fn set_app(&self, app: Option<KickApp>) {
        self.inner.config.lock().await.app = app;
    }

    pub async fn set_account(&self, account: Option<KickAccount>) {
        self.inner.config.lock().await.account = account;
    }

    pub fn on_token_refresh(&self, hook: TokenHook) {
        *self.inner.hook.lock().unwrap() = Some(hook);
    }
}

/// Identifiant numérique Kick attendu par l'API de modération.
fn numeric_user_id(user: &str) -> Result<u64> {
    user.trim().parse().map_err(|_| {
        Error::Other(format!(
            "identifiant Kick invalide ({user}) : l'API de modération exige l'identifiant numérique de l'utilisateur"
        ))
    })
}

/// Corps d'un ban (`seconds` en secondes, converti en minutes arrondies au supérieur, 1 à 10 080).
fn ban_body(broadcaster: u64, user: u64, seconds: Option<u32>, reason: Option<&str>) -> Value {
    let mut body = json!({ "broadcaster_user_id": broadcaster, "user_id": user });
    if let Some(s) = seconds {
        body["duration"] = json!(s.div_ceil(60).clamp(1, 10_080));
    }
    if let Some(r) = reason.filter(|r| !r.is_empty()) {
        body["reason"] = json!(r.chars().take(100).collect::<String>());
    }
    body
}

fn parse_channel(v: &Value, channel_id: &str) -> (Option<u64>, Option<StreamInfo>) {
    let item = &v["data"][0];
    let broadcaster = item["broadcaster_user_id"].as_u64();
    let stream = &item["stream"];
    let info = stream["is_live"]
        .as_bool()
        .unwrap_or(false)
        .then(|| StreamInfo {
            channel_id: channel_id.to_string(),
            title: item["stream_title"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            viewers: stream["viewer_count"].as_u64().unwrap_or(0) as u32,
            started_at: stream["start_time"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        });
    (broadcaster, info)
}

fn parse_categories(v: &Value) -> Vec<Category> {
    v["data"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|c| {
                    Some(Category {
                        id: c["id"].as_u64()?.to_string(),
                        name: c["name"].as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

impl Inner {
    /// Compte valide (jeton renouvelé s'il expire dans moins d'une minute).
    async fn account(&self, force_refresh: bool) -> Result<KickAccount> {
        let mut cfg = self.config.lock().await;
        let app = cfg.app.clone().ok_or(Error::NotAuthenticated("Kick"))?;
        let acc = cfg
            .account
            .as_mut()
            .ok_or(Error::NotAuthenticated("Kick"))?;
        let expiring = acc.tokens.expires_at.is_some_and(|e| e - 60 < now_secs());
        if force_refresh || expiring {
            if let Some(refresh) = acc.tokens.refresh_token.clone() {
                match auth::kick::refresh(&self.http, &app.client_id, &app.client_secret, &refresh)
                    .await
                {
                    Ok(mut fresh) => {
                        fresh.refresh_token = fresh.refresh_token.or(Some(refresh));
                        acc.tokens = fresh;
                        if let Some(hook) = self.hook.lock().unwrap().clone() {
                            hook(&acc.user_id, &acc.tokens);
                        }
                    }
                    Err(_)
                        if !force_refresh
                            && !acc.tokens.expires_at.is_some_and(|e| e < now_secs()) => {}
                    Err(e) => return Err(e),
                }
            }
        }
        Ok(acc.clone())
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<Value>,
    ) -> Result<Value> {
        let mut attempt = 0;
        loop {
            let acc = self.account(attempt > 0).await?;
            let mut req = self
                .http
                .request(method.clone(), format!("{API_URL}/{path}"))
                .query(query)
                .bearer_auth(&acc.tokens.access_token);
            if let Some(b) = &body {
                req = req.json(b);
            }
            let resp = req.send().await?;
            let status = resp.status();
            if status == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                attempt += 1;
                continue;
            }
            let text = resp.text().await?;
            let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            if !status.is_success() {
                let msg = value["message"].as_str().unwrap_or(text.as_str());
                return Err(Error::Other(format!("Kick ({status}): {msg}")));
            }
            return Ok(value);
        }
    }

    /// Réponse `/channels` pour un slug, avec mémorisation de l'identifiant du diffuseur.
    async fn channel(&self, slug: &str, channel_id: &str) -> Result<(u64, Option<StreamInfo>)> {
        let slug = slug.to_lowercase();
        let v = self
            .call(Method::GET, "channels", &[("slug", slug.as_str())], None)
            .await?;
        let (broadcaster, info) = parse_channel(&v, channel_id);
        let id =
            broadcaster.ok_or_else(|| Error::Other(format!("chaîne Kick introuvable: {slug}")))?;
        self.broadcasters.lock().unwrap().insert(slug, id);
        Ok((id, info))
    }

    async fn broadcaster_id(&self, channel: &Channel) -> Result<u64> {
        let slug = channel.external_id.to_lowercase();
        if let Some(id) = self.broadcasters.lock().unwrap().get(&slug) {
            return Ok(*id);
        }
        Ok(self.channel(&slug, &channel.id).await?.0)
    }
}

#[async_trait]
impl ChatConnector for KickConnector {
    fn platform(&self) -> Platform {
        Platform::Kick
    }

    /// Pas de lecture du chat : on signale simplement la chaîne comme prête.
    async fn connect(&self, channel: &Channel, tx: mpsc::Sender<ChatEvent>) -> Result<()> {
        let _ = tx
            .send(ChatEvent::Connected {
                channel_id: channel.id.clone(),
            })
            .await;
        Ok(())
    }

    async fn disconnect(&self, _channel: &Channel) -> Result<()> {
        Ok(())
    }

    async fn send_message(&self, channel: &Channel, text: &str) -> Result<()> {
        let broadcaster = self.inner.broadcaster_id(channel).await?;
        let clean: String = text
            .replace(['\r', '\n'], " ")
            .chars()
            .take(MAX_MESSAGE_CHARS)
            .collect();
        let body = json!({"broadcaster_user_id": broadcaster, "content": clean, "type": "user"});
        self.inner
            .call(Method::POST, "chat", &[], Some(body))
            .await
            .map(|_| ())
    }

    async fn stream_info(&self, channel: &Channel) -> Result<Option<StreamInfo>> {
        if self.inner.account(false).await.is_err() {
            return Ok(None);
        }
        Ok(self
            .inner
            .channel(&channel.external_id, &channel.id)
            .await?
            .1)
    }

    async fn update_stream_info(
        &self,
        _channel: &Channel,
        update: &StreamInfoUpdate,
    ) -> Result<()> {
        // L'API Kick modifie toujours la chaîne du propriétaire du jeton.
        let mut body = json!({});
        if let Some(title) = update.title.as_deref().filter(|t| !t.trim().is_empty()) {
            body["stream_title"] = json!(title.trim());
        }
        if let Some(name) = update.category.as_deref().filter(|c| !c.trim().is_empty()) {
            let found = self.search_categories(name).await?;
            let wanted = name.trim().to_lowercase();
            let category = found
                .iter()
                .find(|c| c.name.to_lowercase() == wanted)
                .or(found.first())
                .ok_or_else(|| Error::Other(format!("catégorie Kick introuvable: {name}")))?;
            body["category_id"] = json!(category
                .id
                .parse::<u64>()
                .map_err(|_| Error::Other("identifiant de catégorie Kick invalide".into()))?);
        }
        if let Some(tags) = &update.tags {
            body["custom_tags"] = json!(tags);
        }
        if body.as_object().is_some_and(|o| o.is_empty()) {
            return Ok(());
        }
        self.inner
            .call(Method::PATCH, "channels", &[], Some(body))
            .await
            .map(|_| ())
    }

    async fn search_categories(&self, query: &str) -> Result<Vec<Category>> {
        if query.trim().is_empty() || self.inner.account(false).await.is_err() {
            return Ok(Vec::new());
        }
        let v = self
            .inner
            .call(Method::GET, "categories", &[("q", query.trim())], None)
            .await?;
        Ok(parse_categories(&v))
    }

    async fn moderate(&self, action: &ModerationAction) -> Result<()> {
        let inner = &self.inner;
        match &action.kind {
            ModerationKind::DeleteMessage { message_id } => inner
                .call(Method::DELETE, &format!("chat/{message_id}"), &[], None)
                .await
                .map(|_| ()),
            ModerationKind::Timeout { user_id, seconds } => {
                let b = inner.broadcaster_id(&action.channel).await?;
                let body = ban_body(b, numeric_user_id(user_id)?, Some(*seconds), None);
                inner
                    .call(Method::POST, "moderation/bans", &[], Some(body))
                    .await
                    .map(|_| ())
            }
            ModerationKind::Ban { user_id, reason } => {
                let b = inner.broadcaster_id(&action.channel).await?;
                let body = ban_body(b, numeric_user_id(user_id)?, None, reason.as_deref());
                inner
                    .call(Method::POST, "moderation/bans", &[], Some(body))
                    .await
                    .map(|_| ())
            }
            ModerationKind::Unban { user_id } => {
                let b = inner.broadcaster_id(&action.channel).await?;
                let body = ban_body(b, numeric_user_id(user_id)?, None, None);
                inner
                    .call(Method::DELETE, "moderation/bans", &[], Some(body))
                    .await
                    .map(|_| ())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_ban_bodies() {
        let perm = ban_body(1, 2, None, Some("spam"));
        assert_eq!(perm["broadcaster_user_id"], 1);
        assert_eq!(perm["user_id"], 2);
        assert_eq!(perm["reason"], "spam");
        assert!(perm.get("duration").is_none());
        // 90 s -> 2 min ; 10 s -> 1 min ; plafonné à 7 jours.
        assert_eq!(ban_body(1, 2, Some(90), None)["duration"], 2);
        assert_eq!(ban_body(1, 2, Some(10), None)["duration"], 1);
        assert_eq!(ban_body(1, 2, Some(u32::MAX), None)["duration"], 10_080);
    }

    #[test]
    fn requires_numeric_ids() {
        assert_eq!(numeric_user_id(" 42 ").unwrap(), 42);
        assert!(numeric_user_id("pseudo").is_err());
    }

    #[test]
    fn parses_channel_and_categories() {
        let v = json!({"data":[{"broadcaster_user_id":7,"slug":"garder","stream_title":"Soirée",
            "stream":{"is_live":true,"viewer_count":12,"start_time":"2026-01-01T10:00:00Z"}}]});
        let (id, info) = parse_channel(&v, "kick:garder");
        let info = info.unwrap();
        assert_eq!(
            (id, info.title.as_str(), info.viewers),
            (Some(7), "Soirée", 12)
        );
        let off = json!({"data":[{"broadcaster_user_id":7,"stream":{"is_live":false}}]});
        assert!(parse_channel(&off, "k").1.is_none());
        let cats =
            parse_categories(&json!({"data":[{"id":5,"name":"Just Chatting"},{"name":"x"}]}));
        assert_eq!(
            cats,
            vec![Category {
                id: "5".into(),
                name: "Just Chatting".into()
            }]
        );
    }
}
