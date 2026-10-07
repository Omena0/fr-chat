# Functional Requirements

## Matrix Account and Sync

- Support homeserver discovery, login using supported Matrix authentication flows, device verification, cross-signing/key backup flows, logout, multiple accounts, and clear recovery guidance.
- Use the Rust Matrix SDK for Client-Server API, sync, encryption, and protocol handling. Keep protocol versions and SDK upgrades behind the Matrix provider boundary.
- Persist sync state and resume after offline periods or process termination. Surface connection state, sync errors, and retry behavior without losing unsent drafts.
- Support account switching without leaking account-scoped room, media, search, notification, or encryption state.

## Messaging and Community Features

- Support spaces, rooms, room directory/join flows, invites, room details, membership changes permitted to the user, and room permissions/power levels.
- Support text, formatted text, replies, threads, edits, redactions, reactions, read markers, typing indicators, link previews where enabled, and media/file attachments.
- Support room and message search within available server capabilities; distinguish local cached results from server results.
- Support drafts, unread counts, mentions, notification preferences, per-room mute settings, and notification routing.
- Support encrypted and unencrypted rooms. Clearly explain when a server/bridge/provider feature cannot preserve end-to-end encryption or when a remote participant is not a verified Matrix device.
- Provide reasonable offline behavior: cached history, drafts, queued supported sends, explicit pending/failed state, and safe retry semantics.

## Mautrix and Bridged Conversations

- Treat a bridged room as a Matrix room for sync, membership, encryption state, and message delivery; use bridge metadata for remote platform context when present.
- Recognize mautrix bridge metadata and ghost users through documented metadata/configuration, with a manual or admin-provided fallback if metadata is absent. Do not rely solely on display-name or user-ID prefix heuristics.
- Keep remote identity mapping, bridge bot/system messages, portal-room context, and double-puppeting status available in channel/user details and diagnostics.
- Render bridge-provided remote names and avatars without changing the underlying Matrix sender or event identity.
- Do not assume remote edits, reactions, threads, read receipts, encryption, or delivery receipts are supported just because the Matrix room supports a corresponding event. Use observed/declared bridge capabilities and report unsupported operations.
- Never claim that a bridged conversation is end-to-end encrypted across the remote platform unless that property is verified end to end.

## Platform-Neutral Extension API

- Define a versioned, documented Rust API and extension-author guide before implementing any third-party platform connector.
- Model an extension as a provider of one or more authorized user accounts, conversations, participants, events, and capability-gated actions. It must be able to represent user-authorized account puppeting where the provider explicitly permits the authentication and actions. Bot accounts are not supported.
- Support provider lifecycle, interactive authorization, account revocation, initial and incremental sync with resumable cursors, event identity/deduplication, conversation discovery, message send, and optional edit/redact, reaction, thread, attachment, typing, read-state, and membership operations.
- Require extensions to declare capabilities, user identity model, required scopes, data handling, and supported conversation kinds. Bridge/system actors may appear as conversation participants, but cannot be connected as bot accounts. The UI adapts to declared capabilities while retaining the same product-level server/channel semantics.
- Keep provider credentials inside the provider/platform credential boundary. The common UI and domain core must never receive raw passwords, access tokens, cookies, or session secrets.
- Require user-visible authorization, revocation, least privilege, rate-limit/backoff handling, and an auditable account/action trail. Do not support bypassing access controls, undocumented private APIs, self-bot behavior where prohibited, or impersonation of an account the user is not authorized to control.
- Namespace remote account, conversation, event, and participant IDs by provider. Preserve origin and mapping metadata so that equal IDs from different systems cannot collide.
- Define graceful behavior for partial support, provider disconnection, rate limits, expired credentials, remote deletions, duplicate/out-of-order events, and provider removal.
- Extension API design and conformance fixtures are in scope; provider implementations, extension hosting/distribution, and runtime third-party plugin loading are not. Do not promise a stable Rust binary ABI. Any later extension runtime requires a separate security and Android feasibility decision.

## Storage, Privacy, and Settings

- Keep account data, sync state, drafts, and cached media partitioned by account and deletable on logout.
- Use OS credential stores: Android Keystore-backed storage and an appropriate Linux Secret Service/libsecret integration, with a documented fallback that does not silently store secrets in plaintext.
- Encrypt sensitive local data where practical; document what Matrix encryption does and does not protect (in particular, local caches and notifications).
- Provide exportable diagnostics with explicit user review and redaction of message bodies, access tokens, and personal identifiers by default.
- Make telemetry opt-in, minimal, documented, and unrelated to message content. Provide a build/runtime option with telemetry disabled.
