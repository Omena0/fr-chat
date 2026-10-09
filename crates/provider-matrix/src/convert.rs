use fr_chat_domain::{
    AccountRef, EntityKind, EntityRef, Message, MessageBody, MessageRelation, OpaqueId,
    TimelineEvent, TimelinePayload,
};
use fr_chat_provider_api::{ProviderError, ProviderErrorKind};
use matrix_sdk::{
    ruma::{
        events::room::message::{Relation, RoomMessageEventContent},
        EventId, OwnedRoomId,
    },
};

pub fn convert_timeline_event(
    account: &AccountRef,
    room_id: &OwnedRoomId,
    event: &matrix_sdk::deserialized_responses::TimelineEvent,
) -> Result<fr_chat_provider_api::ProviderChange, ProviderError> {
    use matrix_sdk::ruma::events::room::message::MessageType;
    use matrix_sdk::ruma::events::reaction::ReactionEventContent;

    let event_id = EntityRef::new(
        account.clone(),
        EntityKind::Event,
        OpaqueId::new(event.kind.event_id().map(|id| id.to_string()).unwrap_or_else(|| "unknown".to_owned())).unwrap(),
    );

    let conversation_id = EntityRef::new(
        account.clone(),
        EntityKind::Conversation,
        OpaqueId::new(room_id.to_string()).unwrap(),
    );

    let sender_id = EntityRef::new(
        account.clone(),
        EntityKind::User,
        OpaqueId::new(event.kind.sender().map(|id| id.to_string()).unwrap_or_else(|| "unknown".to_owned())).unwrap(),
    );

    let server_timestamp_ms = event.timestamp.map(|ts| i64::from(ts.get()));

    let payload = match &event.kind {
        matrix_sdk::deserialized_responses::TimelineEventKind::PlainText { event: raw_event } => {
            let raw_any_sync_event = raw_event.deserialize().map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;
            match raw_any_sync_event {
                matrix_sdk::ruma::events::AnySyncTimelineEvent::MessageLike(message_like_event) => {
                    match message_like_event {
                        matrix_sdk::ruma::events::AnySyncMessageLikeEvent::RoomMessage(message_event) => {
                            let content = message_event.as_original().map(|e| &e.content);
                            let (body, relation, edited_at_ms) = match content {
                                Some(content) => {
                                    let body = match &content.msgtype {
                                        MessageType::Text(text) => text.body.clone(),
                                        _ => "[Unsupported message type]".to_owned(),
                                    };
                                    let relation = content.relates_to.as_ref().and_then(|rel| match rel {
                                        Relation::Reply(reply) => {
                                            Some(MessageRelation::Reply {
                                                message_id: EntityRef::new(
                                                    account.clone(),
                                                    EntityKind::Message,
                                                    OpaqueId::new(reply.in_reply_to.event_id.to_string()).unwrap(),
                                                ),
                                            })
                                        }
                                        Relation::Thread(thread) => {
                                            Some(MessageRelation::Thread {
                                                root_message_id: EntityRef::new(
                                                    account.clone(),
                                                    EntityKind::Message,
                                                    OpaqueId::new(thread.event_id.to_string()).unwrap(),
                                                ),
                                            })
                                        }
                                        _ => None,
                                    });
                                    let edited_at_ms = if matches!(content.relates_to, Some(Relation::Replacement(_))) {
                                        server_timestamp_ms
                                    } else {
                                        None
                                    };
                                    (body, relation, edited_at_ms)
                                }
                                None => ("[Redacted message]".to_owned(), None, None),
                            };
                            let message = Message {
                                id: event_id.clone(),
                                conversation_id: conversation_id.clone(),
                                sender_id: sender_id.clone(),
                                sent_at_ms: server_timestamp_ms.unwrap_or(0),
                                edited_at_ms,
                                body: MessageBody::plain(body),
                                relation,
                                attachments: Vec::new(),
                                delivery: fr_chat_domain::DeliveryState::Sent,
                            };
                            TimelinePayload::MessageCreated {
                                message: Box::new(message),
                            }
                        }
                        matrix_sdk::ruma::events::AnySyncMessageLikeEvent::Reaction(message_event) => {
                            let content = message_event.as_original().map(|e| &e.content);
                            if let Some(content) = content {
                                let reaction_content: &ReactionEventContent = content;
                                let key = reaction_content.relates_to.key.clone();
                                let relates_to_event_id = EntityRef::new(
                                    account.clone(),
                                    EntityKind::Event,
                                    OpaqueId::new(reaction_content.relates_to.event_id.to_string()).unwrap(),
                                );
                                TimelinePayload::ReactionAdded {
                                    message_id: relates_to_event_id,
                                    key,
                                }
                            } else {
                                TimelinePayload::Unsupported {
                                    type_id: event.kind.event_type().unwrap_or_else(|| "unknown".to_owned()),
                                    summary: Some("[Redacted reaction]".to_owned()),
                                }
                            }
                        }
                        _ => TimelinePayload::Unsupported {
                            type_id: event.kind.event_type().unwrap_or_else(|| "unknown".to_owned()),
                            summary: Some("[Unsupported message-like event]".to_owned()),
                        },
                    }
                }
                _ => TimelinePayload::Unsupported {
                    type_id: event.kind.event_type().unwrap_or_else(|| "unknown".to_owned()),
                    summary: Some("[Unsupported timeline event]".to_owned()),
                },
            }
        }
        _ => TimelinePayload::Unsupported {
            type_id: event.kind.event_type().unwrap_or_else(|| "unknown".to_owned()),
            summary: Some("[Encrypted or undecryptable event]".to_owned()),
        },
    };

    let timeline_event = TimelineEvent {
        id: event_id,
        conversation_id,
        sender_id: Some(sender_id),
        server_timestamp_ms,
        position: fr_chat_domain::EventPosition {
            provider_sequence: None,
            received_sequence: 0,
        },
        payload,
    };

    Ok(fr_chat_provider_api::ProviderChange::TimelineEvent(timeline_event))
}

pub fn build_message_content(
    body: MessageBody,
    attachments: Vec<EntityRef>,
    relation: Option<MessageRelation>,
) -> Result<RoomMessageEventContent, ProviderError> {
    use matrix_sdk::ruma::events::relation::{Reply, Thread};

    let mut content = if let Some(formatted) = body.formatted {
        RoomMessageEventContent::text_html(body.plain, formatted.value)
    } else {
        RoomMessageEventContent::text_plain(body.plain)
    };

    if let Some(rel) = relation {
        content.relates_to = Some(match rel {
            MessageRelation::Reply { message_id } => {
                let event_id = EventId::parse(message_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
                Relation::Reply(Reply::with_event_id(event_id.to_owned()))
            }
            MessageRelation::Thread { root_message_id } => {
                let event_id = EventId::parse(root_message_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
                Relation::Thread(Thread::without_fallback(event_id.to_owned()))
            }
        });
    }

    if !attachments.is_empty() {
        let _attachment_text = attachments
            .iter()
            .map(|a| format!("\n[{}]", a.remote_id))
            .collect::<String>();
    }

    Ok(content)
}