use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use fr_chat_app_core::AppCore;
use fr_chat_domain::{AccountRef, EntityRef, ProviderId};
use fr_chat_provider_api::{
    ActionRequest, ApiVersion, AuthResponse, AuthStartRequest,
    CredentialHandle, CredentialStore, ProviderContext, ProviderError, ProviderErrorKind,
    SecureInputHandle, SecureInputVault, SecretBytes,
};
use fr_chat_provider_matrix::auth::MatrixProvider;
use fr_chat_store_sqlite::SqliteAppStore;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct FfiProviderDescriptor {
    provider_id: String,
    display_name: String,
    provider_version: String,
    minimum_api: ApiVersion,
    maximum_api: ApiVersion,
    identity_model: String,
    conversation_kinds: Vec<String>,
    capabilities: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct FfiSyncResult {
    applied_change_count: u64,
    new_events: u64,
    updated_conversations: Vec<String>,
    next_cursor: Option<Vec<u8>>,
    has_more: bool,
}

#[derive(Clone)]
struct NoopSecureInputVault;

#[async_trait]
impl SecureInputVault for NoopSecureInputVault {
    async fn take(&self, _handle: &SecureInputHandle) -> Result<SecretBytes, ProviderError> {
        Err(ProviderError::new(
            ProviderErrorKind::Unsupported,
            "Secure input not supported in FFI".to_owned(),
        ))
    }
}

#[derive(Clone)]
struct NoopCredentialStore;

#[async_trait]
impl CredentialStore for NoopCredentialStore {
    async fn store(
        &self,
        _account: &AccountRef,
        _name: &str,
        _secret: SecretBytes,
    ) -> Result<CredentialHandle, ProviderError> {
        Err(ProviderError::new(
            ProviderErrorKind::Unsupported,
            "Credential store not supported in FFI".to_owned(),
        ))
    }

    async fn load(
        &self,
        _account: &AccountRef,
        _handle: &CredentialHandle,
    ) -> Result<SecretBytes, ProviderError> {
        Err(ProviderError::new(
            ProviderErrorKind::Unsupported,
            "Credential store not supported in FFI".to_owned(),
        ))
    }

    async fn delete(
        &self,
        _account: &AccountRef,
        _handle: &CredentialHandle,
    ) -> Result<(), ProviderError> {
        Err(ProviderError::new(
            ProviderErrorKind::Unsupported,
            "Credential store not supported in FFI".to_owned(),
        ))
    }
}

struct FfiContextFactory {
    base_context: ProviderContext,
}

impl FfiContextFactory {
    fn new() -> Self {
        let base_context = ProviderContext {
            secure_inputs: Arc::new(NoopSecureInputVault),
            credentials: Arc::new(NoopCredentialStore),
        };
        Self { base_context }
    }
}

impl fr_chat_app_core::ProviderContextFactory for FfiContextFactory {
    fn for_provider(&self, provider: &ProviderId) -> ProviderContext {
        self.base_context.scoped_to_provider(provider.clone())
    }
    fn for_account(&self, account: &AccountRef) -> ProviderContext {
        self.base_context.scoped_to_account(account.clone())
    }
}

#[derive(Clone)]
struct FfiEventSink {
    events: Arc<std::sync::Mutex<Vec<fr_chat_app_core::AppEvent>>>,
}

impl FfiEventSink {
    fn new() -> Self {
        Self {
            events: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }
}

impl fr_chat_app_core::AppEventSink for FfiEventSink {
    fn publish(&self, event: fr_chat_app_core::AppEvent) {
        let mut events = self.events.lock().unwrap();
        events.push(event);
    }
}

struct FfiApp {
    core: Arc<AppCore>,
    _events: FfiEventSink,
}

fn decode_json(data: &CStr) -> Result<serde_json::Value, String> {
    let s = data.to_str().map_err(|e| e.to_string())?;
    serde_json::from_str(s).map_err(|e| e.to_string())
}

fn from_json<T: for<'de> Deserialize<'de>>(value: serde_json::Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|e| e.to_string())
}

fn to_json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| e.to_string())
}

fn get_core(app: Arc<std::sync::Mutex<Option<FfiApp>>>) -> Result<Arc<AppCore>, String> {
    let mut guard = app.lock().map_err(|e| e.to_string())?;
    let app_ref = guard.as_mut().ok_or_else(|| "not initialized".to_string())?;
    Ok(app_ref.core.clone())
}

fn get_instance() -> Option<Arc<std::sync::Mutex<Option<FfiApp>>>> {
    let ptr = APP_INSTANCE.load(Ordering::Acquire);
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { Arc::from_raw(ptr) }.clone())
    }
}

static APP_INSTANCE: AtomicPtr<std::sync::Mutex<Option<FfiApp>>> = AtomicPtr::new(std::ptr::null_mut());

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ffi_init(
    homeserver: *const c_char,
    store_path: *const c_char,
) -> *mut c_char {
    let result = (|| -> Result<String, String> {
        if !APP_INSTANCE.load(Ordering::Acquire).is_null() {
            return Err("already initialized".to_string());
        }
        let homeserver = unsafe { ptr_to_str(homeserver)? }.to_string();
        let store_path = unsafe { ptr_to_str(store_path)? }.to_string();

        let store = SqliteAppStore::open_encrypted(PathBuf::from(&store_path), SecretBytes::new(vec![0u8; 32]))
            .map_err(|e| e.to_string())?;

        let provider = MatrixProvider::new(homeserver, store_path);

        let events = FfiEventSink::new();
        let core = AppCore::new(
            Arc::new(store),
            Arc::new(FfiContextFactory::new()),
            Arc::new(events.clone()),
        );

        core.register_provider(Arc::new(provider)).map_err(|e| e.to_string())?;

        let app = FfiApp { core: Arc::new(core), _events: events };
        let arc = Arc::new(std::sync::Mutex::new(Some(app)));
        let leaked = Arc::into_raw(arc) as *mut _;
        APP_INSTANCE.store(leaked, Ordering::Release);
        Ok(json!({"ok": true}).to_string())
    })();
    match result {
        Ok(s) => make_c_string(s),
        Err(e) => make_c_string(json!({"error": e}).to_string()),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ffi_begin_authorization(
    provider_id: *const c_char,
    request_json: *const c_char,
) -> *mut c_char {
    let result = (|| -> Result<String, String> {
        let provider_id = unsafe { ptr_to_str(provider_id)? };
        let request_value = unsafe { decode_json(&c_str(request_json)?)? };
        let request: AuthStartRequest = from_json(request_value)?;
        let app = get_instance().ok_or("not initialized")?;
        let core = get_core(app)?;
        let provider_id_owned = ProviderId::new(provider_id).map_err(|e| e.to_string())?;
        let rt = tokio::runtime::Handle::current();
        let step = rt.block_on(core.begin_authorization(&provider_id_owned, request))
            .map_err(|e| e.to_string())?;
        to_json(&step)
    })();
    match result {
        Ok(s) => make_c_string(s),
        Err(e) => make_c_string(json!({"error": e}).to_string()),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ffi_continue_authorization(
    provider_id: *const c_char,
    response_json: *const c_char,
) -> *mut c_char {
    let result = (|| -> Result<String, String> {
        let provider_id = unsafe { ptr_to_str(provider_id)? };
        let response_value = unsafe { decode_json(&c_str(response_json)?)? };
        let response: AuthResponse = from_json(response_value)?;
        let app = get_instance().ok_or("not initialized")?;
        let core = get_core(app)?;
        let provider_id_owned = ProviderId::new(provider_id).map_err(|e| e.to_string())?;
        let rt = tokio::runtime::Handle::current();
        let step = rt.block_on(core.continue_authorization(&provider_id_owned, response))
            .map_err(|e| e.to_string())?;
        to_json(&step)
    })();
    match result {
        Ok(s) => make_c_string(s),
        Err(e) => make_c_string(json!({"error": e}).to_string()),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ffi_accounts() -> *mut c_char {
    let result = (|| -> Result<String, String> {
        let app = get_instance().ok_or("not initialized")?;
        let core = get_core(app)?;
        let rt = tokio::runtime::Handle::current();
        let accounts = rt.block_on(core.accounts()).map_err(|e| e.to_string())?;
        to_json(&accounts)
    })();
    match result {
        Ok(s) => make_c_string(s),
        Err(e) => make_c_string(json!({"error": e}).to_string()),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ffi_servers(account_ref_json: *const c_char) -> *mut c_char {
    let result = (|| -> Result<String, String> {
        let account_ref_value = unsafe { decode_json(&c_str(account_ref_json)?)? };
        let account_ref: AccountRef = from_json(account_ref_value)?;
        let app = get_instance().ok_or("not initialized")?;
        let core = get_core(app)?;
        let rt = tokio::runtime::Handle::current();
        let servers = rt.block_on(core.servers(&account_ref)).map_err(|e| e.to_string())?;
        to_json(&servers)
    })();
    match result {
        Ok(s) => make_c_string(s),
        Err(e) => make_c_string(json!({"error": e}).to_string()),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ffi_sync(account_ref_json: *const c_char) -> *mut c_char {
    let result = (|| -> Result<String, String> {
        let account_ref_value = unsafe { decode_json(&c_str(account_ref_json)?)? };
        let account_ref: AccountRef = from_json(account_ref_value)?;
        let app = get_instance().ok_or("not initialized")?;
        let core = get_core(app)?;
        let rt = tokio::runtime::Handle::current();
        let result = rt.block_on(core.sync_account(&account_ref, None)).map_err(|e| e.to_string())?;
        let updated_conversations: Vec<String> = result.commit.updated_conversations.iter().map(|e| e.remote_id.to_string()).collect();
        let ffi_result = FfiSyncResult {
            applied_change_count: result.commit.applied_change_count,
            new_events: result.commit.new_events,
            updated_conversations,
            next_cursor: result.next_cursor.map(|c| c.as_bytes().to_vec()),
            has_more: result.has_more,
        };
        to_json(&ffi_result)
    })();
    match result {
        Ok(s) => make_c_string(s),
        Err(e) => make_c_string(json!({"error": e}).to_string()),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ffi_execute(request_json: *const c_char) -> *mut c_char {
    let result = (|| -> Result<String, String> {
        let request_value = unsafe { decode_json(&c_str(request_json)?)? };
        let request: ActionRequest = from_json(request_value)?;
        let app = get_instance().ok_or("not initialized")?;
        let core = get_core(app)?;
        let rt = tokio::runtime::Handle::current();
        let outcome = rt.block_on(core.execute_action(request)).map_err(|e| e.to_string())?;
        to_json(&outcome)
    })();
    match result {
        Ok(s) => make_c_string(s),
        Err(e) => make_c_string(json!({"error": e}).to_string()),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ffi_conversations(account_ref_json: *const c_char) -> *mut c_char {
    let result = (|| -> Result<String, String> {
        let account_ref_value = unsafe { decode_json(&c_str(account_ref_json)?)? };
        let account_ref: AccountRef = from_json(account_ref_value)?;
        let app = get_instance().ok_or("not initialized")?;
        let core = get_core(app)?;
        let rt = tokio::runtime::Handle::current();
        let conversations = rt.block_on(core.conversations(&account_ref, None))
            .map_err(|e| e.to_string())?;
        to_json(&conversations)
    })();
    match result {
        Ok(s) => make_c_string(s),
        Err(e) => make_c_string(json!({"error": e}).to_string()),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ffi_timeline(
    account_ref_json: *const c_char,
    conversation_ref_json: *const c_char,
    limit: u16,
) -> *mut c_char {
    let result = (|| -> Result<String, String> {
        let account_ref_value = unsafe { decode_json(&c_str(account_ref_json)?)? };
        let account_ref: AccountRef = from_json(account_ref_value)?;
        let conversation_ref_value = unsafe { decode_json(&c_str(conversation_ref_json)?)? };
        let conversation_ref: EntityRef = from_json(conversation_ref_value)?;
        let app = get_instance().ok_or("not initialized")?;
        let core = get_core(app)?;
        let rt = tokio::runtime::Handle::current();
        let events = rt.block_on(core.timeline(&account_ref, &conversation_ref, None, limit))
            .map_err(|e| e.to_string())?;
        to_json(&events)
    })();
    match result {
        Ok(s) => make_c_string(s),
        Err(e) => make_c_string(json!({"error": e}).to_string()),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ffi_descriptor() -> *mut c_char {
    let result = (|| -> Result<String, String> {
        let app = get_instance().ok_or("not initialized")?;
        let core = get_core(app)?;
        let descriptors = core.providers().map_err(|e| e.to_string())?;
        let ffi_desc: Vec<FfiProviderDescriptor> = descriptors.iter().map(|d| FfiProviderDescriptor {
            provider_id: d.provider_id.to_string(),
            display_name: d.display_name.clone(),
            provider_version: d.provider_version.clone(),
            minimum_api: d.minimum_api,
            maximum_api: d.maximum_api,
            identity_model: match d.identity_model {
                fr_chat_provider_api::ProviderIdentityModel::UserAccount => "user_account".to_string(),
            },
            conversation_kinds: d.conversation_kinds.iter().map(|k| format!("{:?}", k)).collect(),
            capabilities: d.capabilities.iter().map(|c| format!("{:?}", c)).collect(),
        }).collect();
        to_json(&ffi_desc)
    })();
    match result {
        Ok(s) => make_c_string(s),
        Err(e) => make_c_string(json!({"error": e}).to_string()),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ffi_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe { let _ = CString::from_raw(ptr); }
    }
}

unsafe fn c_str<'a>(ptr: *const c_char) -> Result<&'a CStr, String> {
    if ptr.is_null() {
        return Err("null pointer".to_string());
    }
    let cstr = unsafe { CStr::from_ptr(ptr) };
    if cstr.to_bytes().contains(&0) {
        return Err("embedded null".to_string());
    }
    Ok(cstr)
}

unsafe fn ptr_to_str<'a>(ptr: *const c_char) -> Result<&'a str, String> {
    let cstr = unsafe { c_str(ptr)? };
    cstr.to_str().map_err(|e| e.to_string())
}

fn make_c_string(s: String) -> *mut c_char {
    CString::new(s).unwrap().into_raw()
}