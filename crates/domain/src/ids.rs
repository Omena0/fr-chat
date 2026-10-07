use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum IdError {
    #[error("identifier must not be empty")]
    Empty,
    #[error("identifier exceeds the maximum length of {max} bytes")]
    TooLong { max: usize },
    #[error("identifier contains a control character")]
    ControlCharacter,
    #[error(
        "provider ID must start with a lowercase letter and contain only lowercase letters, digits, '.', '_' or '-'"
    )]
    InvalidProviderId,
}

macro_rules! opaque_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
                let value = value.into();
                if value.is_empty() {
                    return Err(IdError::Empty);
                }
                if value.len() > 255 {
                    return Err(IdError::TooLong { max: 255 });
                }
                if value.chars().any(char::is_control) {
                    return Err(IdError::ControlCharacter);
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

opaque_id!(OpaqueId);

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProviderId(String);

impl ProviderId {
    pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
        let value = value.into();
        if value.is_empty() {
            return Err(IdError::Empty);
        }
        if value.len() > 64 {
            return Err(IdError::TooLong { max: 64 });
        }
        let mut bytes = value.bytes();
        let Some(first) = bytes.next() else {
            return Err(IdError::Empty);
        };
        if !first.is_ascii_lowercase()
            || !bytes.all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
            })
        {
            return Err(IdError::InvalidProviderId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for ProviderId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ProviderId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Server,
    Conversation,
    User,
    Role,
    Message,
    Event,
    Media,
    Other(String),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct AccountRef {
    pub provider_id: ProviderId,
    pub account_id: OpaqueId,
}

impl AccountRef {
    pub fn new(provider_id: ProviderId, account_id: OpaqueId) -> Self {
        Self {
            provider_id,
            account_id,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct EntityRef {
    pub account: AccountRef,
    pub kind: EntityKind,
    pub remote_id: OpaqueId,
}

impl EntityRef {
    pub fn new(account: AccountRef, kind: EntityKind, remote_id: OpaqueId) -> Self {
        Self {
            account,
            kind,
            remote_id,
        }
    }

    pub fn belongs_to(&self, account: &AccountRef) -> bool {
        &self.account == account
    }

    pub fn has_kind(&self, expected: &EntityKind) -> bool {
        &self.kind == expected
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct RemoteOrigin {
    pub provider_id: ProviderId,
    pub kind: EntityKind,
    pub remote_id: OpaqueId,
}

impl RemoteOrigin {
    pub fn new(provider_id: ProviderId, kind: EntityKind, remote_id: OpaqueId) -> Self {
        Self {
            provider_id,
            kind,
            remote_id,
        }
    }
}
