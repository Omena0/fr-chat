# Architecture

## System Boundaries

Use a Rust workspace with strict dependency direction and a shared Kotlin Compose Multiplatform presentation layer. Compose owns rendering, interaction, and platform-adaptive layout; Rust owns provider-independent domain rules, Matrix protocol work, sync, and provider coordination. Keep the Rust/Kotlin boundary narrow and generated where practical. Validate the binding and packaging approach for both Android and the Linux Compose Desktop JVM before committing to it.

```text
Compose Multiplatform UI (Kotlin; Android + Linux Desktop)
             | generated/narrow FFI bindings
Rust application core: domain models, use cases, provider routing, policy
       /                   |                      \
Matrix provider       Extension API          Platform service boundary
       |                   |                    /                \
Matrix SDK       future providers          Android host       Linux host
                                             |                    |
                                      Keystore, intents    Secret Service, DBus
```

## Workspace Boundaries

- `app-core`: provider-agnostic conversation, participant, event, account, capability, sync-state, and navigation models; command/query use cases; application event stream; policy enforcement.
- `extension-api`: versioned provider contract, capability declarations, account authorization/revocation, sync cursors, event identity, action contracts, error model, and test fixtures. Keep the in-process Rust trait API separate from any stable wire/IPC API; no UI toolkit, Matrix SDK, Android, or Linux dependencies.
- `provider-matrix`: Matrix SDK integration, sync, encryption, room/event mapping, search, and Matrix-specific behavior.
- `provider-registry`: provider registration, account lifecycle, capability checks, error isolation, and extension version negotiation.
- `ui-compose`: shared Compose Multiplatform screens, design system, navigation, and responsive workflows. Keep platform-specific UI behavior behind expect/actual or injected host interfaces, following Compose conventions.
- `bindings`: narrow generated Kotlin bindings (UniFFI or an equivalent validated approach) for Rust core commands, state/events, errors, and async operations. Do not expose Rust implementation types as UI contracts.
- `platform-android`: Android host, lifecycle/background work, notification channels, Keystore-backed storage, intents, permissions, and Android-specific integrations.
- `platform-linux`: Compose Desktop/JVM host and packaging, Secret Service, desktop notifications, file chooser, deep links, and Linux desktop integration.
- `app-storage`: schema migrations, account partitioning, cache policy, and persistence interfaces; no provider-specific secrets.

## Dependency and Data-Flow Rules

- Compose UI invokes typed Kotlin-facing core commands and observes typed state/events; it does not call Matrix SDK or provider APIs directly.
- Core routes provider-scoped commands to the owning provider after capability and authorization checks.
- Keep UI presentation state and platform integration in Kotlin; keep shared domain policy, Matrix protocol logic, and provider coordination in Rust. No business rule should be duplicated across the boundary.
- Keep JNI/FFI calls coarse-grained and asynchronous; do not cross the language boundary per timeline row or animation frame. Define cancellation, threading, error translation, and resource-lifetime rules.
- Each conversation/event has a stable provider namespace and source ID. A Matrix room remains Matrix-owned even when a bridge associates it with a remote channel.
- Cross-provider presentation is a view over provider-owned data, not an implicit data migration or identity merge.
- Provider failures are isolated and surfaced in the relevant account/conversation; they must not prevent Matrix sync or another provider from operating.
- Platform services are injected interfaces. Common logic must not branch on Android/Linux except at the shell or platform-service implementation boundary.
