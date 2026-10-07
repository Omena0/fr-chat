# Implementation Plan

## Delivery Strategy

Build the Matrix experience first, while designing a platform-neutral extension contract that keeps future providers possible. Use Compose Multiplatform for Android and Linux Desktop, with a Rust core behind a narrow Kotlin binding layer. Extension implementations, including Discord, are out of scope. Plan the extension API and a Discord extension design/feasibility brief only; do not include Discord data import, bot accounts, or runtime third-party plugin hosting.

## Phase 0: Product and Technical Validation

1. Validate Matrix newcomer journeys using the app's default Discord-familiar server/channel semantics across the product.
2. Build a Compose Multiplatform vertical slice on Android and Linux Desktop: responsive navigation, Android IME/back handling, screen-reader behavior, Linux keyboard/focus behavior, packaging, and timeline performance.
3. Validate the Rust-to-Kotlin binding and async strategy on both targets, including Android lifecycle, JVM native-library loading, cancellation, and error translation.
4. Spike Matrix SDK support for login, sync, encrypted rooms, device verification, key backup, media, and Android lifecycle/background constraints.
5. Document a threat model for local caches, notifications, Matrix keys, future provider credentials, and extension isolation.
6. Decide minimum OS versions, Linux package formats, voice/video scope, and local-data protection strategy.

**Exit criteria:** Compose Multiplatform is demonstrated on both targets; architecture records identify the binding strategy, Matrix SDK baseline, packaging targets, security boundary, and release-one feature cut.

## Phase 1: Workspace and Core Contracts

1. Create the Rust workspace, Kotlin Compose modules, binding layer, and CI for formatting, linting, unit tests, dependency/license audit, Android build, and Linux Desktop build.
2. Define provider-independent domain IDs and models for accounts, servers/spaces, channels/rooms, conversations, participants, events, capabilities, and sync state.
3. Specify the versioned, platform-neutral extension API: user-account authorization/revocation, resumable sync, event identity/deduplication, capability-gated actions, error model, and conformance tests. Do not implement a concrete extension.
4. Define the Kotlin-facing facade and threading/cancellation rules; verify the UI depends only on the facade and provider-independent models.
5. Implement a deterministic in-memory test provider and pass the provider contract suite.
6. Specify provider identity namespacing, event ordering, redaction/deletion, account revocation, credential isolation, and API compatibility/version negotiation. Explicitly exclude bot accounts and platform-specific assumptions from the common contract.

**Exit criteria:** test provider passes contract tests; Compose UI uses the Kotlin facade; provider errors and unsupported capabilities are visible; credentials and provider SDK types do not cross into common UI/domain code.

## Phase 2: Matrix Foundation

1. Integrate the Rust Matrix SDK behind `provider-matrix`.
2. Implement homeserver discovery, supported login flows, account lifecycle, sync persistence, reconnect, logout, and multi-account isolation.
3. Implement device verification, cross-signing/key backup, encrypted-room state, key storage, and recoverable error flows.
4. Implement spaces, room membership/join, room details, timeline pagination, text messages, replies, edits, redactions, reactions, read markers, typing, and attachments.
5. Add local schema migrations, draft persistence, cache eviction, and crash/restart recovery.

**Exit criteria:** integration tests cover login/logout, encrypted and unencrypted rooms, sync interruption/restart, account isolation, event deduplication, and key/device recovery paths.

## Phase 3: Compose Multiplatform Client

1. Implement the agreed responsive information architecture and design system: community rail, channel navigation, direct messages, timeline, composer, and optional member pane.
2. Add desktop keyboard navigation and shortcuts, Android back/IME behavior, adaptive navigation, accessible focus and screen-reader semantics, reduced motion, and scalable text.
3. Add unread state, mentions, notification preferences, search, drafts, offline indicators, and explicit pending/failed send state.
4. Integrate Android secure storage, notifications, permissions, lifecycle, and share intents.
5. Integrate Linux keyring, desktop notifications, file chooser, deep links, window behavior, and agreed packaging formats.
6. Add light/dark themes and verify visual distinction from Discord's branding.

**Exit criteria:** end-to-end user journeys pass on a representative Android device/emulator and supported Linux desktop environments; accessibility and responsive checks pass at agreed viewport sizes.

## Phase 4: Mautrix and Bridge Quality

1. Implement bridge metadata parsing and room/user origin models using documented metadata, with a configurable fallback.
2. Present bridged channels, ghost users, bridge/system events, remote identity mapping, and double-puppeting state without altering underlying Matrix event identity.
3. Show provider origin and capabilities in channel/user details and explain unsupported actions at point of use.
4. Test rooms with missing metadata, multiple bridge bots, remote-name/avatar changes, ghost-user collisions, encrypted rooms, and disconnected/limited bridges.
5. Add diagnostics that identify the Matrix room and bridge/provider state without collecting message content or secrets.

**Exit criteria:** representative mautrix bridges work without special user setup where metadata is available; unsupported bridge behavior is explicit; no false end-to-end encryption claims.

## Phase 5: Extension API and Discord Design (Planning Only)

1. Complete a focused feasibility and policy review of Discord user-account puppeting and existing mautrix-discord integration options. Do not treat bot accounts as an alternative.
2. Write a Discord extension design brief mapping servers, channels, DMs, participants, messages, attachments, reactions, threads, and read state to the provider-neutral model, with capability gaps clearly listed.
3. Specify how a future Discord extension would authorize/revoke a user account, isolate credentials, handle sync cursors, rate limits, identity mapping, remote deletion, and provider diagnostics.
4. Validate that Discord-specific concepts and transport details do not leak into the common extension API or force changes to the app's default server/channel semantics.
5. If no permitted and technically supportable user-account access path exists, document the blocker and keep the extension as a design-only proposal. Do not implement or ship an extension in this plan.

**Exit criteria:** the generic API specification and conformance tests are complete; the Discord design brief documents supported user-account access feasibility, capability mapping, and unresolved blockers. No provider implementation is produced.

## Phase 6: Hardening and Release

1. Complete security review, dependency audit, secret/logging audit, local-data deletion tests, and extension boundary review.
2. Add load/performance tests for large rooms, long timelines, media-heavy rooms, and poor/offline connectivity.
3. Test upgrade and downgrade-safe failure behavior for local schema changes, plus backup/recovery and logout deletion.
4. Finalize privacy policy, telemetry opt-in/default-off behavior, diagnostics export, release signing, update channels, and support workflow.
5. Produce signed Android release artifacts and Linux packages; run packaging, install, upgrade, and rollback smoke tests.

**Exit criteria:** acceptance criteria in the product spec are met on both platforms; release blockers from security, accessibility, and reliability reviews are closed or explicitly accepted.

## Phase 7: Future Extension Implementations (Out of Scope)

Any implementation, hosting, packaging, signing, sandboxing, or third-party distribution of providers requires a separately approved project scope. This plan delivers only the platform-neutral API design and Discord extension design brief.

## Cross-Cutting Work and Required Gates

- Every provider feature includes capability tests, authorization/revocation tests, error handling, and identity-collision tests.
- Every UI milestone is checked on narrow Android and wide/narrow Linux layouts, with keyboard, touch, screen-reader, and reduced-motion paths.
- Every credential or persistence change includes account isolation, logout deletion, migration, and log/crash-redaction tests.
- Keep protocol, Compose/Rust binding, packaging, platform-neutral extension API, and Discord user-account feasibility in short architecture decision records.

## Initial Backlog (Priority Order)

1. Compose Multiplatform Android/Linux vertical slice and Rust binding proof of concept.
2. Threat model and credential/local-cache decision.
3. Rust/Kotlin workspace, CI, domain model, extension API, and provider contract suite.
4. Matrix login/sync/encryption proof of concept.
5. Responsive navigation and timeline vertical slice.
6. Messaging, offline/draft behavior, notifications, and search.
7. Mautrix metadata and ghost-user handling.
8. Platform-neutral extension API conformance suite and Discord extension design/feasibility brief; no provider implementation.
9. Accessibility, performance, packaging, hardening, and release readiness.

## Open Decisions

- Kotlin/Rust binding and async bridge selection for Android and Compose Desktop/JVM; prove packaging and lifecycle behavior with a vertical slice.
- Minimum supported Android API level, Linux distributions/package formats, and CPU architectures.
- Required Matrix authentication flows and homeserver compatibility baseline.
- Whether first release includes calls/voice/video or defers them; Matrix call support has meaningful protocol and platform scope.
- Offline retention limits, local database encryption approach, and backup/export policy.
- Future extension runtime, distribution, isolation, review, and signing model. No extension runtime is part of this implementation scope.
- Discord user-account access feasibility and policy constraints. Bot accounts are not a fallback; if a permitted user-authorized access path cannot be established, the Discord extension remains a design-only proposal.
- Exact origin-details placement and labels after a usability test with Matrix newcomers and bridged-community users.
