use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

use crate::{AccountRef, CapabilityMap, EntityKind, EntityRef, RemoteOrigin};

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ModelError {
    #[error("{field} has the wrong entity kind")]
    WrongKind { field: &'static str },
    #[error("{field} belongs to a different account")]
    AccountMismatch { field: &'static str },
    #[error("field must not be empty: {0}")]
    EmptyField(&'static str),
    #[error("formatted body must include plain text")]
    FormattedBodyWithoutPlainText,
}

fn validate_ref(
    value: &EntityRef,
    account: &EntityRef,
    expected: EntityKind,
    field: &'static str,
) -> Result<(), ModelError> {
    if !value.has_kind(&expected) {
        return Err(ModelError::WrongKind { field });
    }
    if !value.belongs_to(&account.account) {
        return Err(ModelError::AccountMismatch { field });
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Connecting,
    Connected,
    Reconnecting,
    Offline,
    AuthorizationRequired,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SyncState {
    pub connection: ConnectionState,
    pub last_successful_sync_ms: Option<i64>,
    pub pending_operations: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountRef,
    pub user_id: Option<EntityRef>,
    pub display_name: String,
    pub avatar: Option<EntityRef>,
    pub sync: SyncState,
    #[serde(default)]
    pub credential_refs: BTreeMap<String, crate::OpaqueId>,
}

impl Account {
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.display_name.trim().is_empty() {
            return Err(ModelError::EmptyField("display_name"));
        }
        if self.credential_refs.keys().any(|name| name.trim().is_empty()) {
            return Err(ModelError::EmptyField("credential_refs.name"));
        }
        if let Some(user) = &self.user_id {
            if !user.has_kind(&EntityKind::User) {
                return Err(ModelError::WrongKind { field: "user_id" });
            }
            if !user.belongs_to(&self.id) {
                return Err(ModelError::AccountMismatch { field: "user_id" });
            }
        }
        if let Some(avatar) = &self.avatar {
            if !avatar.has_kind(&EntityKind::Media) {
                return Err(ModelError::WrongKind { field: "avatar" });
            }
            if !avatar.belongs_to(&self.id) {
                return Err(ModelError::AccountMismatch { field: "avatar" });
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationKind {
    Text,
    DirectMessage,
    GroupDirectMessage,
    Category,
    Forum,
    Voice,
    Other(String),
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct UnreadState {
    pub unread_count: u64,
    pub mention_count: u64,
    pub last_read_event_id: Option<EntityRef>,
}

impl UnreadState {
    pub fn validate(&self, conversation: &EntityRef) -> Result<(), ModelError> {
        if let Some(event) = &self.last_read_event_id {
            validate_ref(event, conversation, EntityKind::Event, "last_read_event_id")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Server {
    pub id: EntityRef,
    pub parent_id: Option<EntityRef>,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<EntityRef>,
    pub origin: Option<RemoteOrigin>,
    pub capabilities: CapabilityMap,
    pub position: Option<i64>,
}

impl Server {
    pub fn validate(&self) -> Result<(), ModelError> {
        if !self.id.has_kind(&EntityKind::Server) {
            return Err(ModelError::WrongKind { field: "id" });
        }
        if self.name.trim().is_empty() {
            return Err(ModelError::EmptyField("name"));
        }
        if let Some(parent) = &self.parent_id {
            validate_ref(parent, &self.id, EntityKind::Server, "parent_id")?;
        }
        if let Some(icon) = &self.icon {
            validate_ref(icon, &self.id, EntityKind::Media, "icon")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Conversation {
    pub id: EntityRef,
    pub server_id: Option<EntityRef>,
    pub parent_id: Option<EntityRef>,
    pub kind: ConversationKind,
    pub name: String,
    pub topic: Option<String>,
    pub origin: Option<RemoteOrigin>,
    pub capabilities: CapabilityMap,
    pub position: Option<i64>,
    pub unread: UnreadState,
}

impl Conversation {
    pub fn validate(&self) -> Result<(), ModelError> {
        if !self.id.has_kind(&EntityKind::Conversation) {
            return Err(ModelError::WrongKind { field: "id" });
        }
        if self.name.trim().is_empty()
            && !matches!(
                self.kind,
                ConversationKind::DirectMessage | ConversationKind::GroupDirectMessage
            )
        {
            return Err(ModelError::EmptyField("name"));
        }
        if let Some(server) = &self.server_id {
            validate_ref(server, &self.id, EntityKind::Server, "server_id")?;
        }
        if let Some(parent) = &self.parent_id {
            validate_ref(parent, &self.id, EntityKind::Conversation, "parent_id")?;
        }
        self.unread.validate(&self.id)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Role {
    pub id: EntityRef,
    pub server_id: EntityRef,
    pub name: String,
    pub color: Option<String>,
    pub position: Option<i64>,
    pub managed: bool,
}

impl Role {
    pub fn validate(&self) -> Result<(), ModelError> {
        if !self.id.has_kind(&EntityKind::Role) {
            return Err(ModelError::WrongKind { field: "id" });
        }
        validate_ref(&self.server_id, &self.id, EntityKind::Server, "server_id")?;
        if self.name.trim().is_empty() {
            return Err(ModelError::EmptyField("name"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: EntityRef,
    pub display_name: String,
    pub username: Option<String>,
    pub avatar: Option<EntityRef>,
    pub origin: Option<RemoteOrigin>,
}

impl User {
    pub fn validate(&self) -> Result<(), ModelError> {
        if !self.id.has_kind(&EntityKind::User) {
            return Err(ModelError::WrongKind { field: "id" });
        }
        if self.display_name.trim().is_empty() {
            return Err(ModelError::EmptyField("display_name"));
        }
        if let Some(avatar) = &self.avatar {
            validate_ref(avatar, &self.id, EntityKind::Media, "avatar")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Member {
    pub user_id: EntityRef,
    pub server_id: Option<EntityRef>,
    pub conversation_id: Option<EntityRef>,
    pub role_ids: Vec<EntityRef>,
    pub joined_at_ms: Option<i64>,
}

impl Member {
    pub fn validate(&self) -> Result<(), ModelError> {
        if !self.user_id.has_kind(&EntityKind::User) {
            return Err(ModelError::WrongKind { field: "user_id" });
        }
        for (reference, expected, field) in [
            (self.server_id.as_ref(), EntityKind::Server, "server_id"),
            (
                self.conversation_id.as_ref(),
                EntityKind::Conversation,
                "conversation_id",
            ),
        ] {
            if let Some(reference) = reference {
                if !reference.has_kind(&expected) {
                    return Err(ModelError::WrongKind { field });
                }
                if !reference.belongs_to(&self.user_id.account) {
                    return Err(ModelError::AccountMismatch { field });
                }
            }
        }
        for role in &self.role_ids {
            validate_ref(role, &self.user_id, EntityKind::Role, "role_ids")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FormattedBody {
    pub format: String,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MessageBody {
    pub plain: String,
    pub formatted: Option<FormattedBody>,
}

impl MessageBody {
    pub fn plain(value: impl Into<String>) -> Self {
        Self {
            plain: value.into(),
            formatted: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub id: EntityRef,
    pub filename: String,
    pub content_type: Option<String>,
    pub byte_length: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl Attachment {
    pub fn validate(&self, message: &EntityRef) -> Result<(), ModelError> {
        validate_ref(&self.id, message, EntityKind::Media, "attachment.id")?;
        if self.filename.trim().is_empty() {
            return Err(ModelError::EmptyField("attachment.filename"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "data")]
pub enum MessageRelation {
    Reply { message_id: EntityRef },
    Thread { root_message_id: EntityRef },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "error")]
pub enum DeliveryState {
    Draft,
    Queued,
    Sending,
    Sent,
    Failed { retryable: bool },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: EntityRef,
    pub conversation_id: EntityRef,
    pub sender_id: EntityRef,
    pub sent_at_ms: i64,
    pub edited_at_ms: Option<i64>,
    pub body: MessageBody,
    pub relation: Option<MessageRelation>,
    pub attachments: Vec<Attachment>,
    pub delivery: DeliveryState,
}

impl Message {
    pub fn validate(&self) -> Result<(), ModelError> {
        if !self.id.has_kind(&EntityKind::Message) {
            return Err(ModelError::WrongKind { field: "id" });
        }
        if !self.conversation_id.has_kind(&EntityKind::Conversation) {
            return Err(ModelError::WrongKind {
                field: "conversation_id",
            });
        }
        if !self.sender_id.has_kind(&EntityKind::User) {
            return Err(ModelError::WrongKind { field: "sender_id" });
        }
        if !self.id.belongs_to(&self.conversation_id.account)
            || !self.sender_id.belongs_to(&self.conversation_id.account)
        {
            return Err(ModelError::AccountMismatch {
                field: "message references",
            });
        }
        if self.body.formatted.is_some() && self.body.plain.is_empty() {
            return Err(ModelError::FormattedBodyWithoutPlainText);
        }
        if let Some(relation) = &self.relation {
            let reference = match relation {
                MessageRelation::Reply { message_id } => message_id,
                MessageRelation::Thread { root_message_id } => root_message_id,
            };
            validate_ref(reference, &self.id, EntityKind::Message, "relation")?;
        }
        for attachment in &self.attachments {
            attachment.validate(&self.id)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SpaceChildOrder(pub String);

impl SpaceChildOrder {
    pub fn new(order: String) -> Self {
        Self(order)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SpaceChildOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for SpaceChildOrder {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_owned()))
    }
}

/// Push rule kinds as defined by the Matrix specification
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PushRuleKind {
    Override,
    Underride,
    Sender,
    Room,
    Content,
}

/// Push rule conditions
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum PushCondition {
    EventMatch { key: String, pattern: String },
    ContainsDisplayName,
    RoomMemberCount { is: String },
    SenderNotificationPermission { key: String },
}

/// Push rule actions
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum PushAction {
    Notify,
    DontNotify,
    SetTweaks { tweaks: PushTweaks },
    Coalesce,
}

/// Push tweaks for notification appearance
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct PushTweaks {
    pub sound: Option<String>,
    pub highlight: Option<bool>,
    pub set_tweak: Option<String>,
}

/// A complete push rule
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PushRule {
    pub rule_id: String,
    pub kind: PushRuleKind,
    pub enabled: bool,
    pub conditions: Vec<PushCondition>,
    pub actions: Vec<PushAction>,
    pub default: bool,
}

/// Push ruleset containing all rule kinds
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct PushRuleset {
    pub override_rules: Vec<PushRule>,
    pub underride_rules: Vec<PushRule>,
    pub sender_rules: Vec<PushRule>,
    pub room_rules: Vec<PushRule>,
    pub content_rules: Vec<PushRule>,
}

/// Pusher configuration for push gateways (APNs, FCM, etc.)
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Pusher {
    pub pushkey: String,
    pub kind: PusherKind,
    pub app_id: Option<String>,
    pub app_display_name: Option<String>,
    pub device_display_name: Option<String>,
    pub profile_tag: Option<String>,
    pub lang: Option<String>,
    pub data: serde_json::Value,
}

/// Pusher kinds
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PusherKind {
    Http,
    Email,
    Apns,
    Fcm,
}

/// Push notification payload
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PushNotification {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub sender_display_name: Option<String>,
    pub room_name: Option<String>,
    pub room_alias: Option<String>,
    pub content: serde_json::Value,
    pub counts: PushCounts,
    pub devices: Vec<PushDevice>,
    pub profile_tag: Option<String>,
}

/// Push notification counts
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct PushCounts {
    pub unread: u32,
    pub missed_calls: u32,
}

/// Device for push notification delivery
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PushDevice {
    pub pushkey: String,
    pub pushkind: PusherKind,
    pub app_id: Option<String>,
    pub data: serde_json::Value,
    pub tweaks: PushTweaks,
}

/// Search event keys as defined by MSC2551
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchEventKey {
    ContentBody,
    ContentName,
    ContentTopic,
    Other(String),
}

/// Search order by options
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchOrderBy {
    Recent,
    Rank,
    Other(String),
}

/// Search event filter
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct SearchEventFilter {
    pub rooms: Vec<String>,
    pub not_rooms: Vec<String>,
    pub senders: Vec<String>,
    pub not_senders: Vec<String>,
    pub types: Vec<String>,
    pub not_types: Vec<String>,
    pub contains_url: Option<bool>,
}

/// Search event context
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SearchEventContext {
    pub before_limit: u32,
    pub after_limit: u32,
    pub include_profile: bool,
}

impl Default for SearchEventContext {
    fn default() -> Self {
        Self {
            before_limit: 5,
            after_limit: 5,
            include_profile: false,
        }
    }
}

/// Search criteria for room events
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SearchCriteria {
    pub search_term: String,
    pub keys: Option<Vec<SearchEventKey>>,
    pub filter: Option<SearchEventFilter>,
    pub order_by: Option<SearchOrderBy>,
    pub event_context: Option<SearchEventContext>,
    pub include_state: Option<bool>,
}

/// Search categories (only room_events for now)
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct SearchCategories {
    pub room_events: Option<SearchCriteria>,
}

/// Search result group
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SearchResultGroup {
    pub next_batch: Option<String>,
    pub order: Option<u32>,
    pub results: Vec<String>, // event IDs
}

/// Search result groups by key
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SearchResultGroups {
    RoomId(std::collections::BTreeMap<String, SearchResultGroup>),
    Sender(std::collections::BTreeMap<String, SearchResultGroup>),
}

/// Search result for a single event
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    pub context: Option<SearchEventContextResult>,
    pub rank: Option<f64>,
    pub result: Option<serde_json::Value>, // Raw event
}

/// Search event context result
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct SearchEventContextResult {
    pub end: Option<String>,
    pub events_after: Vec<serde_json::Value>,
    pub events_before: Vec<serde_json::Value>,
    pub profile_info: std::collections::BTreeMap<String, SearchUserProfile>,
    pub start: Option<String>,
}

/// Search user profile
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SearchUserProfile {
    pub avatar_url: Option<String>,
    pub display_name: Option<String>,
}

/// Search results for room events
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct SearchRoomEventsResult {
    pub count: Option<u32>,
    pub groups: Option<SearchResultGroups>,
    pub next_batch: Option<String>,
    pub results: Vec<SearchResult>,
    pub state: std::collections::BTreeMap<String, Vec<serde_json::Value>>,
    pub highlights: Vec<String>,
}

/// Complete search response
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct SearchResponse {
    pub room_events: SearchRoomEventsResult,
}

/// VoIP call events (MSC2746, MSC3401)
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoipCallType {
    Voice,
    Video,
}

/// VoIP session description (SDP)
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoipSessionDescription {
    pub sdp: String,
    pub sdp_type: VoipSdpType,
}

/// SDP type
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VoipSdpType {
    Offer,
    Answer,
    Pranswer,
    Rollback,
}

/// VoIP ICE candidate
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoipIceCandidate {
    pub candidate: String,
    pub sdp_mid: String,
    pub sdp_mline_index: u32,
    pub username_fragment: Option<String>,
}

/// VoIP version
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoipVersion {
    Version0,
    Version1,
    Other(String),
}

/// m.call.invite event
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoipInviteEventContent {
    pub call_id: String,
    pub party_id: Option<String>,
    pub offer: VoipSessionDescription,
    pub lifetime: Option<u64>,
    pub version: VoipVersion,
    pub call_type: Option<VoipCallType>,
}

/// m.call.candidates event
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoipCandidatesEventContent {
    pub call_id: String,
    pub party_id: Option<String>,
    pub candidates: Vec<VoipIceCandidate>,
    pub version: VoipVersion,
}

/// m.call.answer event
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoipAnswerEventContent {
    pub call_id: String,
    pub party_id: Option<String>,
    pub answer: VoipSessionDescription,
    pub version: VoipVersion,
}

/// m.call.hangup event
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoipHangupEventContent {
    pub call_id: String,
    pub party_id: Option<String>,
    pub reason: Option<VoipHangupReason>,
    pub version: VoipVersion,
}

/// Hangup reasons
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoipHangupReason {
    UserHangup,
    CallTimeout,
    AnsweredElsewhere,
    IceFailed,
    ConnectionFailed,
    MediaFailed,
    Other(String),
}

/// m.call.reject event
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoipRejectEventContent {
    pub call_id: String,
    pub party_id: Option<String>,
    pub reason: Option<VoipRejectReason>,
    pub version: VoipVersion,
}

/// Reject reasons
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoipRejectReason {
    Busy,
    Rejected,
    CallTimeout,
    UnsupportedCallType,
    Other(String),
}

/// m.call.select_answer event (for glare resolution)
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoipSelectAnswerEventContent {
    pub call_id: String,
    pub party_id: Option<String>,
    pub selected_party_id: Option<String>,
    pub version: VoipVersion,
}

/// m.call.negotiate event (renegotiation)
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoipNegotiateEventContent {
    pub call_id: String,
    pub party_id: Option<String>,
    pub offer: VoipSessionDescription,
    pub lifetime: Option<u64>,
    pub version: VoipVersion,
}

/// VoIP call state in the application
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoipCallState {
    Invited,
    Ringing,
    Connected,
    Ended,
    Failed,
}

/// VoIP call representation
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoipCall {
    pub id: EntityRef,
    pub conversation_id: EntityRef,
    pub call_id: String,
    pub initiator_id: EntityRef,
    pub call_type: VoipCallType,
    pub state: VoipCallState,
    pub created_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub offer: Option<VoipSessionDescription>,
    pub answer: Option<VoipSessionDescription>,
    pub candidates: Vec<VoipIceCandidate>,
}
