# Core Contract and Implementation Design

## Purpose

The core is the provider-neutral application layer between platform providers and the Compose UI. It owns canonical app semantics, authorization/capability policy, event ordering and deduplication, durable state transitions, and errors. It must not know about Matrix SDK types, Matrix room IDs, Compose, Android, Linux, or any concrete platform extension.

This contract is the implementation boundary for the Matrix provider and future platform shells. A change in provider or UI must not require redefining conversation, message, account, or synchronization semantics.

## Ownership and Dependency Direction

```text
Compose UI and platform shells
              |
       Kotlin binding facade
              |
         app-core API
        /           \
provider-api       storage/platform ports
      |
provider-matrix (later; outside this core slice)
```

- `domain`: provider-neutral models, typed identifiers, capability vocabulary, invariants, and validated actions. No async runtime, protocol SDK, database, JNI, or UI dependencies.
- `provider-api`: provider metadata, account authorization lifecycle, sync and action contracts. It depends on domain types only and does not implement a provider.
- `app-core`: use-case orchestration, provider registry, authorization/capability checks, durable sync/outbox rules, and ports for persistence and credentials.
- `bindings`: a separate Kotlin-facing adapter. It maps app-core operations and state changes into a versioned, coarse-grained Kotlin API; it does not expose provider SDK objects or become the source of domain rules.
- `provider-matrix`, database adapters, Android/Linux host services, and Compose UI are later layers. They depend inward and cannot be required to compile or test the core.

Do not promise Rust binary ABI stability. Version source-level/provider contracts and any future wire/binding schema independently.

## Canonical Identity Model

- Every remote identity is scoped by `provider_id`, `account_id`, and provider-owned `remote_id`. This applies to servers, channels, users, events, attachments, and cursors.
- A local entity reference is therefore never a bare Matrix room ID or remote platform ID. Equality, lookup, deduplication, and persistence all use the complete scoped reference.
- Account records contain display/profile data, provider identity, connection/sync state, and an opaque credential reference. Access tokens, passwords, cookies, encryption keys, and authorization responses are not domain fields and never appear in UI-facing state or diagnostics.
- A user identity is distinct from an account identity. A bridged/ghost user keeps its Matrix identity and remote identity mapping as separate scoped references; display-name equality never merges identities.
- Preserve provider-owned source IDs and timestamps alongside normalized app fields. Normalization must not destroy data needed to render, diagnose, or round-trip supported actions.

## Product Semantics

The shared UI speaks Discord-familiar product language by default: servers, channels, direct messages, threads, members, and roles where a provider supports them. Those are canonical presentation concepts, not claims about a provider's native model.

- A server is a navigable grouping. A provider may supply one or more nesting levels; the core preserves parent relationships and does not require every channel to belong to a server.
- A channel/conversation has a stable kind (text, direct message, group direct message, category, forum, voice, or other) plus declared capabilities. The kind describes presentation intent; capabilities determine which operations are available.
- Members and role assignments are provider-owned projections. The core does not infer Matrix power levels or remote permissions from names or role labels.
- A Matrix space can map to a server-like grouping, a room to a channel, and a DM room to a direct conversation. Mautrix metadata can attach remote origin and identity mappings without changing the Matrix room or event owner.
- Unknown kinds, fields, and capabilities must round-trip or degrade to an explicit unknown/unsupported representation rather than failing deserialization or being silently discarded.

## Conversation and Event Model

The core models a timeline as ordered provider events, not just rendered message strings. It must represent:

- Message creation, edits, redactions/deletions, replies, thread roots, reactions, attachments, and server/system events.
- Participant and membership changes, typing state, read markers, and provider connection/sync state.
- Event origin, stable scoped event identity, sender identity, server timestamp, receive/order metadata, and relation to the conversation.
- Message content as structured plain/formatted text and typed attachment metadata; rendering and untrusted HTML sanitization belong at explicit boundaries.
- Local delivery state: draft, queued, sending, sent/accepted, failed/retryable, and failed/permanent. Remote acceptance is not equivalent to guaranteed cross-platform delivery.

Edits and redactions are represented as new provider events or projections with their source identities preserved. The core does not rewrite history in a way that loses audit/source identity. Provider semantics decide whether an action maps to a new event, an edit, or a deletion.

## Capabilities and Authorization

Capabilities are declared by the provider and scoped to the account, server, channel, participant, or message they govern. They are descriptive affordances, not a replacement for server-side authorization.

- Model supported, unsupported, and temporarily unavailable operations distinctly from denied/unauthorized operations.
- Include the user-facing operations required by the product: send, edit-own, edit-any, redact-own, redact-any, reply/thread, react, upload, search, mark-read, typing, join/leave, and relevant management operations.
- Include read-only/list capabilities for servers, channels, members, roles, history, and search results.
- Re-check provider-side authorization on every mutation. A capability snapshot may be stale; remote permission rejection is a normal typed failure that updates available state.
- Do not add bot-account identity or authorization modes. Bridge bots/system senders may appear as remote conversation events but are not connectable accounts.
- Use stable capability identifiers and forward-compatible unknown values; never assume a capability solely from conversation kind or provider brand.

## Provider API Contract

The provider contract describes a compiled provider implementation; it does not load, sandbox, distribute, or execute third-party extensions.

Each provider declares a stable provider ID, API compatibility range, data/identity model, authorization scopes and flow, supported conversation kinds, and operation capabilities. Its interface must cover:

1. Provider metadata and compatibility negotiation.
2. User-account authorization, account discovery, reauthorization, credential-reference management, and revocation.
3. Initial and incremental sync with opaque resumable cursors, bounded batches, cancellation, retry/backoff, and typed provider errors.
4. Conversation/member/history discovery and provider events with stable scoped identities.
5. Capability-gated actions for messages, replies/threads, reactions, attachments, read state, typing, membership, and search as supported.
6. Provider health/connection state and diagnostics that contain no credentials or message bodies by default.

The provider does not receive arbitrary UI or platform objects. Credential access is through a constrained vault port; the UI sees only authorization presentation data safe to display and an opaque operation handle when interaction must return to the provider.

## Persistence and Sync Invariants

Persistence is a core port, not a concrete database choice. A storage implementation must provide account partitioning, schema versioning/migrations, and transactions with these guarantees:

- Apply a sync batch's normalized events, deduplication identities, next cursor, and derived sync state atomically. A crash leaves either the old cursor and no batch, or the complete batch and new cursor.
- Replaying a batch or event is idempotent. Event uniqueness is provider + account + source event identity, not content or timestamp.
- Preserve provider event order where supplied. When the provider only supplies partial ordering, represent uncertainty rather than inventing a global order from arrival time.
- Keep sync cursors, encryption stores, drafts, outbox entries, and cached content partitioned by account and deleted according to explicit logout policy.
- Local schema migrations are versioned, tested, and fail closed without destroying recoverable account data.

The core queues user mutations in an outbox with a stable local operation ID/idempotency key. It records state transitions and retries only when the provider's action semantics make retry safe. Permanent authorization/validation failures are not automatically retried. Drafts remain separate from queued sends.

## Platform Ports

Core use cases depend on narrow interfaces, with implementations injected by platform shells:

- `CredentialVault`: store/read/delete opaque credential material by scoped account reference; never log or return secrets to UI.
- `AppStore`: transactional account, conversation, event, cursor, draft, outbox, and preference operations.
- `MediaStore`: staged upload/download handles, bounded streaming, and cache lifecycle; never require full media blobs in memory.
- `Clock` and `IdGenerator`: deterministic time and local operation identity for testability.
- `NotificationSink`: publish already-authorized, privacy-filtered notification intents; OS permission and delivery belong to host adapters.
- `BackgroundWork` and `DeepLinkRouter`: scheduling, cancellation, and intent routing belong to platform hosts, not domain rules.

No platform implementation may change a core invariant. Android and Linux adapters must pass the same port contract tests.

## Kotlin Boundary

Compose calls coarse-grained app operations and observes state snapshots/deltas through a Kotlin facade. It never calls provider traits or storage directly.

- Async operations support cancellation, typed errors, and explicit progress/connection states.
- UI-facing IDs and records have stable serialization/schema versions; Rust internal structs do not cross the ABI directly.
- Event observation is lifecycle-aware and back-pressured/coalesced for replaceable state such as typing, while durable events are not dropped.
- No one-call-per-row, per-keystroke, or per-animation-frame JNI path. Timeline windows and batched updates cross the boundary as collections.
- Secret-bearing authentication and media data do not enter logs, crash reports, analytics, or ordinary UI state.

The actual binding technology is selected by a Phase 0 vertical slice on Android and Linux Desktop before the binding crate/API is frozen.

## Error Model

Use typed, actionable error categories: validation, unsupported capability, authorization required/denied, rate limited (with retry hint), offline/transient, conflict/stale state, remote rejection, storage/migration, credential unavailable/revoked, cancellation, and internal fault. Preserve a safe user message separately from redacted diagnostic context. Never expose raw provider response bodies or credentials in UI errors.

## Build Order and Completion Gates

1. **Contract:** settle canonical models, capability vocabulary, provider and store ports, event ordering/deduplication, outbox safety, and Kotlin boundary through this document and API tests.
2. **Domain crate:** implement typed IDs, models, capability evaluation, and validation with exhaustive unit tests. It must compile without platform or Matrix SDK dependencies.
3. **Provider API crate:** define provider/account/sync/action contracts and conformance fixtures; no concrete provider implementation.
4. **Application core crate:** implement provider registration, authorization lifecycle orchestration, capability checks, atomic sync application, account isolation, and safe outbox state machine against ports.
5. **Storage adapter:** choose and implement the durable store only after transaction and migration tests can prove the invariants above.
6. **Matrix provider:** integrate the Rust Matrix SDK behind the provider contract, including encryption/key store boundaries and mautrix metadata.
7. **Binding vertical slice:** expose real core account/conversation/timeline commands to Compose through the selected Kotlin binding and verify cancellation/lifecycle on both targets.
8. **Platform adapters and UI:** add Android/Linux services and user workflows against the completed facade and port conformance suites.

Do not advance a layer by filling it with placeholder implementations. A layer is complete only when its contracts, failure behavior, and tests pass, even if the next layer has not yet been started.

## Risks to Validate Early

- Rust async provider trait object and cancellation shape with the selected Matrix SDK/runtime.
- Whether UniFFI or an alternative binding can expose batched async state changes cleanly on Android and JVM desktop.
- Matrix SDK store integration with Android Keystore and Linux Secret Service without leaking token/key material into common persistence.
- Mautrix metadata formats and capability discovery: missing metadata must remain supported without heuristic identity merging.
- Provider-specific permission/capability snapshots may be stale; mutations must still fail safely and refresh UI state.
