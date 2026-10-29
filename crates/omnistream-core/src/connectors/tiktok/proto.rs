//! Sous-ensemble des messages protobuf « webcast » de TikTok LIVE nécessaires à la lecture du chat.
//! Les numéros de champ proviennent des schémas publiés par `tiktok-live-proto` ; les champs
//! inconnus sont ignorés au décodage, ce qui rend le sous-ensemble tolérant aux évolutions.

use std::collections::HashMap;

#[derive(Clone, PartialEq, prost::Message)]
pub struct PushHeader {
    #[prost(string, tag = "1")]
    pub key: String,
    #[prost(string, tag = "2")]
    pub value: String,
}

/// Trame binaire échangée sur le websocket.
#[derive(Clone, PartialEq, prost::Message)]
pub struct PushFrame {
    #[prost(int64, tag = "1")]
    pub seq_id: i64,
    #[prost(int64, tag = "2")]
    pub log_id: i64,
    #[prost(int64, tag = "3")]
    pub service: i64,
    #[prost(int64, tag = "4")]
    pub method: i64,
    #[prost(message, repeated, tag = "5")]
    pub headers: Vec<PushHeader>,
    #[prost(string, tag = "6")]
    pub payload_encoding: String,
    #[prost(string, tag = "7")]
    pub payload_type: String,
    #[prost(bytes = "vec", tag = "8")]
    pub payload: Vec<u8>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct BaseMessage {
    #[prost(string, tag = "1")]
    pub method: String,
    #[prost(bytes = "vec", tag = "2")]
    pub payload: Vec<u8>,
}

/// Lot de messages (contenu d'une trame `msg`, aussi renvoyé par le serveur de signature).
#[derive(Clone, PartialEq, prost::Message)]
pub struct FetchResult {
    #[prost(message, repeated, tag = "1")]
    pub messages: Vec<BaseMessage>,
    #[prost(string, tag = "2")]
    pub cursor: String,
    #[prost(string, tag = "5")]
    pub internal_ext: String,
    #[prost(map = "string, string", tag = "7")]
    pub route_params: HashMap<String, String>,
    #[prost(bool, tag = "9")]
    pub need_ack: bool,
    #[prost(string, tag = "10")]
    pub push_server: String,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct Text {
    #[prost(string, tag = "1")]
    pub key: String,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct Common {
    #[prost(int64, tag = "2")]
    pub msg_id: i64,
    #[prost(int64, tag = "4")]
    pub create_time: i64,
    #[prost(message, optional, tag = "8")]
    pub display_text: Option<Text>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct User {
    #[prost(int64, tag = "1")]
    pub id: i64,
    #[prost(string, tag = "3")]
    pub nickname: String,
    /// Pseudo @ de l'utilisateur.
    #[prost(string, tag = "38")]
    pub display_id: String,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct UserIdentity {
    #[prost(bool, tag = "2")]
    pub is_subscriber_of_anchor: bool,
    #[prost(bool, tag = "5")]
    pub is_moderator_of_anchor: bool,
    #[prost(bool, tag = "6")]
    pub is_anchor: bool,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct ChatMessage {
    #[prost(message, optional, tag = "1")]
    pub common: Option<Common>,
    #[prost(message, optional, tag = "2")]
    pub user: Option<User>,
    #[prost(string, tag = "3")]
    pub content: String,
    #[prost(message, optional, tag = "18")]
    pub user_identity: Option<UserIdentity>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct Gift {
    #[prost(int32, tag = "11")]
    pub kind: i32,
    #[prost(int32, tag = "12")]
    pub diamond_count: i32,
    #[prost(string, tag = "16")]
    pub name: String,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct GiftMessage {
    #[prost(message, optional, tag = "1")]
    pub common: Option<Common>,
    #[prost(int32, tag = "5")]
    pub repeat_count: i32,
    #[prost(message, optional, tag = "7")]
    pub user: Option<User>,
    #[prost(int32, tag = "9")]
    pub repeat_end: i32,
    #[prost(message, optional, tag = "15")]
    pub gift: Option<Gift>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct SocialMessage {
    #[prost(message, optional, tag = "1")]
    pub common: Option<Common>,
    #[prost(message, optional, tag = "2")]
    pub user: Option<User>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct SubNotifyMessage {
    #[prost(message, optional, tag = "1")]
    pub common: Option<Common>,
    #[prost(message, optional, tag = "2")]
    pub user: Option<User>,
    #[prost(int64, tag = "4")]
    pub sub_month: i64,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct ControlMessage {
    #[prost(int32, tag = "2")]
    pub action: i32,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct HeartBeat {
    #[prost(int64, tag = "1")]
    pub room_id: i64,
    #[prost(int64, tag = "2")]
    pub send_packet_seq_id: i64,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct EnterRoom {
    #[prost(int64, tag = "1")]
    pub room_id: i64,
    #[prost(int64, tag = "4")]
    pub live_id: i64,
    #[prost(string, tag = "5")]
    pub identity: String,
    #[prost(string, tag = "6")]
    pub cursor: String,
    #[prost(int64, tag = "7")]
    pub account_type: i64,
    #[prost(int64, tag = "8")]
    pub enter_unique_id: i64,
    #[prost(string, tag = "9")]
    pub filter_welcome_msg: String,
}
