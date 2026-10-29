use jiff::SignedDuration;
use jiff::Timestamp;
use toasty::Deferred;
use uuid::Uuid;

pub const LAST_USED_PRECISION: SignedDuration = SignedDuration::from_hours(1);

#[derive(Debug, toasty::Model)]
pub struct Admin {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub name: String,
    pub password_hash: String,
}

#[derive(Debug, toasty::Model)]
pub struct AdminSession {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub admin_id: Uuid,
    #[belongs_to]
    pub admin: Deferred<Admin>,
    #[unique]
    pub token_hash: Vec<u8>,
    pub signed_in_at: Timestamp,
    pub last_used_at: Timestamp,
}

impl AdminSession {
    pub const IDLE_TIMEOUT: SignedDuration = SignedDuration::from_hours(12);
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, serde::Serialize, serde::Deserialize,
)]
#[column(rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub enum Colour {
    Amber,
    Blue,
    Pink,
    Emerald,
}

impl Colour {
    pub const ALL: [Self; 4] = [Self::Amber, Self::Blue, Self::Pink, Self::Emerald];
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, serde::Serialize, serde::Deserialize,
)]
#[column(rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub enum Symbol {
    Ring,
    Triangle,
    Star,
    Waves,
}

impl Symbol {
    pub const ALL: [Self; 4] = [Self::Ring, Self::Triangle, Self::Star, Self::Waves];
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, serde::Serialize, serde::Deserialize,
)]
#[column(rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub enum ClientKind {
    Web,
    AppleTv,
    GoogleTv,
}

#[derive(Debug, toasty::Model)]
pub struct User {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub name: String,
    #[unique]
    pub email: String,
    pub password_hash: String,
    pub colour: Colour,
    pub symbol: Symbol,
}

#[derive(Debug, toasty::Model)]
pub struct Session {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub user_id: Uuid,
    #[belongs_to]
    pub user: Deferred<User>,
    pub client_kind: ClientKind,
    pub device_name: String,
    #[unique]
    pub token_hash: Vec<u8>,
    pub signed_in_at: Timestamp,
    pub last_used_at: Timestamp,
}

impl Session {
    pub const WEB_IDLE_TIMEOUT: SignedDuration = SignedDuration::from_hours(30 * 24);
}

#[derive(Debug, toasty::Model)]
pub struct Invite {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub email: String,
    #[unique]
    pub token_hash: Vec<u8>,
    pub admin_id: Uuid,
    pub sent_at: Timestamp,
    pub expires_at: Timestamp,
}

impl Invite {
    pub const VALID_FOR: SignedDuration = SignedDuration::from_hours(7 * 24);
}
