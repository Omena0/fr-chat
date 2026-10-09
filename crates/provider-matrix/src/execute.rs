use fr_chat_provider_api::{ActionOutcome, ActionRequest, ProviderAction, ProviderContext, ProviderError, ProviderErrorKind};
use fr_chat_domain::{EntityKind, EntityRef, OpaqueId};
use matrix_sdk::ruma::{
        events::{
            reaction::ReactionEventContent,
            relation::Annotation,
            room::message::ReplacementMetadata,
        },
        OwnedServerName, RoomId, EventId,
    };
use ruma_client_api::receipt::create_receipt::v3::ReceiptType;
use ruma_common::SpaceChildOrder as RumaSpaceChildOrder;
use ruma_events::receipt::ReceiptThread;
use mime::Mime;

use crate::convert::build_message_content;

use crate::auth::MatrixProvider;

pub async fn execute(
    provider: &MatrixProvider,
    _context: &ProviderContext,
    request: ActionRequest,
) -> Result<ActionOutcome, ProviderError> {
    let client = provider.get_or_create_client().await?;

    match request.action {
        ProviderAction::SendMessage { conversation_id, body, attachments, relation } => {
            let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

            let room = client
                .get_room(&room_id)
                .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Room not found".to_owned()))?;

            let content = build_message_content(body, attachments, relation)?;
            let event_id = room.send(content).await
                .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

            Ok(ActionOutcome::Accepted {
                event_id: Some(EntityRef::new(
                    request.account.clone(),
                    EntityKind::Event,
                    OpaqueId::new(event_id.response.event_id.to_string()).expect("valid opaque id"),
                )),
            })
        }
        ProviderAction::EditMessage { conversation_id, message_id, body } => {
            let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
            let event_id = EventId::parse(message_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

            let room = client
                .get_room(&room_id)
                .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Room not found".to_owned()))?;

            let content = build_message_content(body, Vec::new(), None)?;
            let replacement_content = content.make_replacement(ReplacementMetadata::new(event_id, None));
            room.send(replacement_content).await
                .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

            Ok(ActionOutcome::Accepted { event_id: None })
        }
        ProviderAction::RedactMessage { conversation_id, message_id, reason } => {
            let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
            let event_id = EventId::parse(message_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

            let room = client
                .get_room(&room_id)
                .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Room not found".to_owned()))?;

            room.redact(&event_id, reason.as_deref(), None).await
                .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

            Ok(ActionOutcome::Accepted { event_id: None })
        }
        ProviderAction::AddReaction { conversation_id, message_id, key } => {
            let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
            let event_id = EventId::parse(message_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

            let room = client
                .get_room(&room_id)
                .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Room not found".to_owned()))?;

            let annotation = Annotation::new(event_id.to_owned(), key);
            let content = ReactionEventContent::new(annotation);
            room.send(content).await
                .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

            Ok(ActionOutcome::Accepted { event_id: None })
        }
        ProviderAction::RemoveReaction { conversation_id, message_id, key } => {
            let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
            let event_id = EventId::parse(message_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

            let room = client
                .get_room(&room_id)
                .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Room not found".to_owned()))?;

            let annotation = Annotation::new(event_id.to_owned(), key);
            let content = ReactionEventContent::new(annotation);
            room.send(content).await
                .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

            Ok(ActionOutcome::Accepted { event_id: None })
        }
        ProviderAction::MarkRead { conversation_id, event_id } => {
            let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

            let room = client
                .get_room(&room_id)
                .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Room not found".to_owned()))?;

            let event_id = EventId::parse(event_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

            room.send_single_receipt(ReceiptType::Read, ReceiptThread::Unthreaded, event_id.to_owned()).await
                .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

            Ok(ActionOutcome::Accepted { event_id: None })
        }
        ProviderAction::SetTyping { conversation_id, typing } => {
            let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

            let room = client
                .get_room(&room_id)
                .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Room not found".to_owned()))?;

            room.typing_notice(typing).await
                .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

            Ok(ActionOutcome::Accepted { event_id: None })
        }
        ProviderAction::JoinConversation { conversation_id } => {
            let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

            client.join_room_by_id(&room_id).await
                .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

            Ok(ActionOutcome::Accepted { event_id: None })
        }
        ProviderAction::LeaveConversation { conversation_id } => {
            let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

            let room = client
                .get_room(&room_id)
                .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Room not found".to_owned()))?;

            room.leave().await
                .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

            Ok(ActionOutcome::Accepted { event_id: None })
        }
        ProviderAction::UploadMedia { conversation_id: _, filename: _filename, content_type, data } => {
            let mime: Mime = content_type.parse()
                .map_err(|e: mime::FromStrError| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

            let media = client.media();
            let upload_response = media.upload(
                &mime,
                data,
                None,
            ).await
                .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

            let mxc_uri = upload_response.content_uri.to_string();
            let media_id = EntityRef::new(
                request.account.clone(),
                EntityKind::Media,
                OpaqueId::new(mxc_uri.clone()).unwrap(),
            );

            Ok(ActionOutcome::Accepted {
                event_id: Some(media_id),
            })
        }
        ProviderAction::CreateSpace {
            name,
            topic: _,
            avatar: _,
            parent_space_id: _,
            initial_children: _,
        } => {
            let mut creation_content = matrix_sdk::ruma::api::client::room::create_room::v3::CreationContent::new();
            creation_content.room_type = Some(matrix_sdk::ruma::room::RoomType::Space);
            let creation_content = Some(matrix_sdk::ruma::serde::Raw::new(&creation_content).unwrap());

            let mut create_room_request = matrix_sdk::ruma::api::client::room::create_room::v3::Request::new();
            create_room_request.name = Some(name.clone());
            create_room_request.creation_content = creation_content;

            let response = client
                .send(create_room_request)
                .await
                .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;
            
            let room_id = response.room_id;
            
            // Mark the room as a space by sending m.room.space state event
            let room = client
                .get_room(&room_id)
                .ok_or_else(|| ProviderError::new(ProviderErrorKind::Internal, "Room not found after creation".to_owned()))?;
            
            room.send_state_event_raw("m.room.space", "", serde_json::json!({}))
                .await
                .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

            let space_id = EntityRef::new(
                request.account.clone(),
                EntityKind::Conversation,
                OpaqueId::new(room_id.to_string()).unwrap(),
            );

            Ok(ActionOutcome::Accepted {
                event_id: Some(space_id),
            })
        }
        ProviderAction::AddSpaceChild {
            space_id,
            child_id,
            order,
            suggested,
        } => {
            let space_room_id = RoomId::parse(space_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
            let child_room_id = RoomId::parse(child_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

            let space_room = client
                .get_room(&space_room_id)
                .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Space not found".to_owned()))?;

            let via = vec![OwnedServerName::try_from(provider.homeserver())
                .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?];

            let mut content = matrix_sdk::ruma::events::space::child::SpaceChildEventContent::new(via);
            if let Some(order) = order {
                content.order = Some(RumaSpaceChildOrder::parse(&order.0).unwrap());
            }
            content.suggested = suggested;


            space_room
                .send_state_event_raw("m.space.child", child_room_id.as_str(), serde_json::to_value(&content).unwrap())
                .await
                .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

            Ok(ActionOutcome::Accepted { event_id: None })
        }
        ProviderAction::RemoveSpaceChild {
            space_id,
            child_id,
        } => {
            let space_room_id = RoomId::parse(space_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
            let child_room_id = RoomId::parse(child_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

            let space_room = client
                .get_room(&space_room_id)
                .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Space not found".to_owned()))?;

            space_room
                .send_state_event_raw("m.space.child", child_room_id.as_str(), serde_json::json!({}))
                .await
                .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

            Ok(ActionOutcome::Accepted { event_id: None })
        }
        ProviderAction::GetPushRules { .. } => {
            // TODO: Fetch push rules from account data or server
            Ok(ActionOutcome::Accepted { event_id: None })
        }
        ProviderAction::SetPushRule { rule_kind: _, rule_id: _, rule: _ } => {
            Ok(ActionOutcome::Accepted { event_id: None })
        }
        ProviderAction::DeletePushRule { rule_kind: _, rule_id: _ } => {
            Ok(ActionOutcome::Accepted { event_id: None })
        }
        ProviderAction::AddPusher { pusher: _ } => {
            Ok(ActionOutcome::Accepted { event_id: None })
        }
        ProviderAction::RemovePusher { pushkey: _, kind: _ } => {
            Ok(ActionOutcome::Accepted { event_id: None })
        }
        ProviderAction::SetPusher { pushkey: _, kind: _, pusher: _ } => {
            Ok(ActionOutcome::Accepted { event_id: None })
        }
        ProviderAction::SearchEvents { criteria: _, next_batch: _ } => {
            Err(ProviderError::new(ProviderErrorKind::Unsupported, "Search not implemented".to_owned()))
        }
        ProviderAction::VoipInvite {
            conversation_id,
            call_id,
            call_type,
            offer,
            lifetime,
        } => {
            let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
            let room = client
                .get_room(&room_id)
                .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Conversation not found".to_owned()))?;

            let sdp_type_str = match offer.sdp_type {
                fr_chat_domain::VoipSdpType::Offer => "offer",
                fr_chat_domain::VoipSdpType::Answer => "answer",
                fr_chat_domain::VoipSdpType::Pranswer => "pranswer",
                fr_chat_domain::VoipSdpType::Rollback => "rollback",
            };

            let version_str = "1";

            let call_type_str = match call_type {
                fr_chat_domain::VoipCallType::Voice => "m.voice",
                fr_chat_domain::VoipCallType::Video => "m.video",
            };

            let content = serde_json::json!({
                "call_id": call_id,
                "offer": {
                    "sdp": offer.sdp,
                    "type": sdp_type_str,
                },
                "lifetime": lifetime,
                "version": version_str,
                "call_type": call_type_str,
            });

            let response = room
                .send_raw("m.call.invite", content)
                .await
                .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

            Ok(ActionOutcome::Accepted {
                event_id: Some(EntityRef::new(
                    request.account.clone(),
                    EntityKind::Event,
                    OpaqueId::new(response.response.event_id.to_string()).unwrap(),
                )),
            })
        }
        ProviderAction::VoipCandidates {
            conversation_id,
            call_id,
            candidates,
        } => {
            let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
            let room = client
                .get_room(&room_id)
                .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Conversation not found".to_owned()))?;

            let candidates_json: Vec<serde_json::Value> = candidates
                .into_iter()
                .map(|c| {
                    let mut obj = serde_json::json!({
                        "candidate": c.candidate,
                        "sdp_mid": c.sdp_mid,
                        "sdp_mline_index": c.sdp_mline_index,
                    });
                    if let Some(ufrag) = c.username_fragment {
                        obj["username_fragment"] = serde_json::Value::String(ufrag);
                    }
                    obj
                })
                .collect();

            let content = serde_json::json!({
                "call_id": call_id,
                "candidates": candidates_json,
                "version": "1",
            });

            let response = room
                .send_raw("m.call.candidates", content)
                .await
                .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

            Ok(ActionOutcome::Accepted {
                event_id: Some(EntityRef::new(
                    request.account.clone(),
                    EntityKind::Event,
                    OpaqueId::new(response.response.event_id.to_string()).unwrap(),
                )),
            })
        }
        ProviderAction::VoipAnswer {
            conversation_id,
            call_id,
            answer,
        } => {
            let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
            let room = client
                .get_room(&room_id)
                .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Conversation not found".to_owned()))?;

            let sdp_type_str = match answer.sdp_type {
                fr_chat_domain::VoipSdpType::Offer => "offer",
                fr_chat_domain::VoipSdpType::Answer => "answer",
                fr_chat_domain::VoipSdpType::Pranswer => "pranswer",
                fr_chat_domain::VoipSdpType::Rollback => "rollback",
            };

            let content = serde_json::json!({
                "call_id": call_id,
                "answer": {
                    "sdp": answer.sdp,
                    "type": sdp_type_str,
                },
                "version": "1",
            });

            let response = room
                .send_raw("m.call.answer", content)
                .await
                .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

            Ok(ActionOutcome::Accepted {
                event_id: Some(EntityRef::new(
                    request.account.clone(),
                    EntityKind::Event,
                    OpaqueId::new(response.response.event_id.to_string()).unwrap(),
                )),
            })
        }
        ProviderAction::VoipHangup {
            conversation_id,
            call_id,
            reason,
        } => {
            let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
let room = client
                .get_room(&room_id)
                .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Conversation not found".to_owned()))?;

            let reason_str: String = reason.map_or(String::new(), |r| match r {
                fr_chat_domain::VoipHangupReason::UserHangup => "user_request".to_owned(),
                fr_chat_domain::VoipHangupReason::CallTimeout => "ice_timeout".to_owned(),
                fr_chat_domain::VoipHangupReason::AnsweredElsewhere => "remote_hangup".to_owned(),
                fr_chat_domain::VoipHangupReason::IceFailed => "ice_failed".to_owned(),
                fr_chat_domain::VoipHangupReason::ConnectionFailed => "connection_failed".to_owned(),
                fr_chat_domain::VoipHangupReason::MediaFailed => "media_failed".to_owned(),
                fr_chat_domain::VoipHangupReason::Other(s) => s.clone(),
            });

            let content = serde_json::json!({
                "call_id": call_id,
                "reason": reason_str,
                "version": "1",
            });

            let response = room
                .send_raw("m.call.hangup", content)
                .await
                .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

            Ok(ActionOutcome::Accepted {
                event_id: Some(EntityRef::new(
                    request.account.clone(),
                    EntityKind::Event,
                    OpaqueId::new(response.response.event_id.to_string()).unwrap(),
                )),
            })
        }
        ProviderAction::VoipReject {
            conversation_id,
            call_id,
            reason,
        } => {
            let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
            let room = client
                .get_room(&room_id)
                .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Conversation not found".to_owned()))?;

            let reason_str: String = reason.map_or(String::new(), |r| match r {
                fr_chat_domain::VoipRejectReason::Busy => "busy".to_owned(),
                fr_chat_domain::VoipRejectReason::Rejected => "rejected".to_owned(),
                fr_chat_domain::VoipRejectReason::CallTimeout => "ice_timeout".to_owned(),
                fr_chat_domain::VoipRejectReason::UnsupportedCallType => "unsupported_call_type".to_owned(),
                fr_chat_domain::VoipRejectReason::Other(s) => s.clone(),
            });

            let content = serde_json::json!({
                "call_id": call_id,
                "reason": reason_str,
                "version": "1",
            });

            let response = room
                .send_raw("m.call.reject", content)
                .await
                .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

            Ok(ActionOutcome::Accepted {
                event_id: Some(EntityRef::new(
                    request.account.clone(),
                    EntityKind::Event,
                    OpaqueId::new(response.response.event_id.to_string()).unwrap(),
                )),
            })
        }
        ProviderAction::VoipSelectAnswer {
            conversation_id,
            call_id,
        } => {
            let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
            let room = client
                .get_room(&room_id)
                .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Conversation not found".to_owned()))?;

            let content = serde_json::json!({
                "call_id": call_id,
                "version": "1",
            });

            let response = room
                .send_raw("m.call.select_answer", content)
                .await
                .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

            Ok(ActionOutcome::Accepted {
                event_id: Some(EntityRef::new(
                    request.account.clone(),
                    EntityKind::Event,
                    OpaqueId::new(response.response.event_id.to_string()).unwrap(),
                )),
            })
        }
        ProviderAction::VoipNegotiate {
            conversation_id,
            call_id,
            ..
        } => {
            let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
            let room = client
                .get_room(&room_id)
                .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Conversation not found".to_owned()))?;

            let content = serde_json::json!({
                "call_id": call_id,
                "version": "1",
            });

            let response = room
                .send_raw("m.call.negotiate", content)
                .await
                .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

            Ok(ActionOutcome::Accepted {
                event_id: Some(EntityRef::new(
                    request.account.clone(),
                    EntityKind::Event,
                    OpaqueId::new(response.response.event_id.to_string()).unwrap(),
                )),
            })
        }
    }
}