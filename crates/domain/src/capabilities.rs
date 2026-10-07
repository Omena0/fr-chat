use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    ListServers,
    ListConversations,
    ListMembers,
    ReadHistory,
    Search,
    SendMessage,
    EditOwnMessage,
    EditAnyMessage,
    RedactOwnMessage,
    RedactAnyMessage,
    Reply,
    Thread,
    React,
    UploadAttachment,
    MarkRead,
    Typing,
    JoinConversation,
    LeaveConversation,
    ManageConversation,
    ManageMembers,
    CreateSpace,
    ManageSpace,
    PushRules,
    PushNotifications,
    Encrypt,
    Decrypt,
    VerifyIdentity,
    Other(String),
}

impl Capability {
    pub fn as_str(&self) -> &str {
        match self {
            Self::ListServers => "list_servers",
            Self::ListConversations => "list_conversations",
            Self::ListMembers => "list_members",
            Self::ReadHistory => "read_history",
            Self::Search => "search",
            Self::SendMessage => "send_message",
            Self::EditOwnMessage => "edit_own_message",
            Self::EditAnyMessage => "edit_any_message",
            Self::RedactOwnMessage => "redact_own_message",
            Self::RedactAnyMessage => "redact_any_message",
            Self::Reply => "reply",
            Self::Thread => "thread",
            Self::React => "react",
            Self::UploadAttachment => "upload_attachment",
            Self::MarkRead => "mark_read",
            Self::Typing => "typing",
            Self::JoinConversation => "join_conversation",
            Self::LeaveConversation => "leave_conversation",
            Self::ManageConversation => "manage_conversation",
            Self::ManageMembers => "manage_members",
            Self::CreateSpace => "create_space",
            Self::ManageSpace => "manage_space",
            Self::PushRules => "push_rules",
            Self::PushNotifications => "push_notifications",
            Self::Encrypt => "encrypt",
            Self::Decrypt => "decrypt",
            Self::VerifyIdentity => "verify_identity",
            Self::Other(value) => value,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    Allowed,
    Denied,
    Unsupported,
    TemporarilyUnavailable,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CapabilityMap(BTreeMap<Capability, CapabilityState>);

impl CapabilityMap {
    pub fn set(&mut self, capability: Capability, state: CapabilityState) {
        self.0.insert(capability, state);
    }

    pub fn state(&self, capability: &Capability) -> CapabilityState {
        self.0
            .get(capability)
            .copied()
            .unwrap_or(CapabilityState::Unsupported)
    }

    pub fn allows(&self, capability: &Capability) -> bool {
        self.state(capability) == CapabilityState::Allowed
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Capability, &CapabilityState)> {
        self.0.iter()
    }
}
