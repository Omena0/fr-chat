use fr_chat_domain::{Pusher, PusherKind};
use fr_chat_provider_api::ProviderError;
use matrix_sdk::{
    ruma::{
        api::client::push::{PusherIds, PusherInit, PusherKind as RumaPusherKind},
        push::HttpPusherData,
    },
};

pub fn convert_domain_pusher_to_init(pusher: Pusher) -> Result<PusherInit, ProviderError> {
    let kind = convert_domain_pusher_kind(pusher.kind)?;
    let app_id = "fr-chat.push".to_owned();
    let ids = PusherIds::new(pusher.pushkey, app_id);
    
    let init = PusherInit {
        ids,
        app_display_name: pusher.app_display_name.unwrap_or_else(|| "fr-chat".to_owned()),
        kind,
        lang: pusher.lang.unwrap_or_else(|| "en".to_owned()),
        device_display_name: pusher.device_display_name.unwrap_or_else(|| "fr-chat".to_owned()),
        profile_tag: pusher.profile_tag,
    };
    
    Ok(init)
}

fn convert_domain_pusher_kind(kind: PusherKind) -> Result<RumaPusherKind, ProviderError> {
    match kind {
        PusherKind::Http => Ok(RumaPusherKind::Http(HttpPusherData::new("https://push.example.com".to_owned()))),
        PusherKind::Apns => Ok(RumaPusherKind::Http(HttpPusherData::new("apns://".to_owned()))),
        PusherKind::Fcm => Ok(RumaPusherKind::Http(HttpPusherData::new("fcm://".to_owned()))),
        PusherKind::Email => Ok(RumaPusherKind::Http(HttpPusherData::new("mailto:".to_owned()))),
    }
}