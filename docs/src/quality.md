# Security, Quality, and Acceptance

## Security and Reliability Requirements

- Threat-model local database/cache access, stolen devices, malicious room content, compromised homeservers, malicious or buggy extensions, notification leakage, and credential revocation.
- Use the Matrix SDK's supported cryptographic implementation; do not implement Matrix cryptography in application code.
- Verify signatures and validate untrusted event/provider data at boundaries; render remote content safely and limit resource use for media and previews.
- Use least-privilege provider capabilities, explicit authorization, revocation, and separate credential storage. A provider must not be able to access another provider's secrets or accounts.
- Apply bounded retries with backoff, idempotency keys where possible, cancellation, sync cursor persistence, and observable error states.
- Do not log message bodies, encryption keys, access tokens, cookies, or raw authorization responses.
- Define backup, cache eviction, database migration, crash recovery, and logout deletion behavior before release.

## Quality Attributes and Release Gates

- **Responsive:** usable from narrow Android portrait through large Linux windows; no clipped controls, unusable pane widths, or loss of navigation context.
- **Accessible:** keyboard-only desktop use, TalkBack/screen-reader semantics, scalable text, contrast checks, focus order, and reduced motion.
- **Reliable:** restart-safe sync and drafts; duplicate/out-of-order event handling; graceful reconnect and credential-expiry recovery.
- **Secure:** threat model reviewed; credential storage verified on both targets; encryption and device-verification flows tested; no secrets in logs or crash reports.
- **Performant:** virtualize long timelines and member lists; bound image/preview work; establish startup, sync, timeline-scroll, memory, and battery budgets in the prototype and CI/device tests.
- **Maintainable:** provider-independent core tests; provider contract conformance suite; migration tests for local schemas; reproducible Linux and Android builds.
- **Shippable:** signed Android artifacts, Linux packages for agreed target formats, update/signing strategy, privacy documentation, support/diagnostics path, and release rollback plan.

## Initial Release Acceptance Criteria

- A user can sign into a Matrix account, verify a device, browse spaces/rooms, and send/receive supported messages on Android and Linux.
- Encrypted-room state and key/device recovery are understandable and tested; the app never silently downgrades security.
- Core Discord-like navigation works at compact and wide widths, with desktop keyboard navigation and Android back/IME behavior.
- Mautrix room and ghost-user metadata is handled without changing Matrix event identity; origin and capability details are available from the relevant submenu/details view.
- Unsupported bridge actions are not falsely presented as successful; bridge and Matrix failures are distinguishable.
- A test provider can be registered through the extension API and pass contract tests for account authorization, sync, identity namespacing, capability-gated actions, errors, revocation, and credential isolation.
- A platform-neutral extension API specification and conformance suite define user-account authorization, capability negotiation, sync, revocation, and credential isolation without requiring a concrete extension implementation.
- A separate Discord extension design brief defines desired semantics and capabilities, user-authorized identity requirements, mautrix-bridge coexistence, and feasibility risks. No Discord connector, bot account, Discord data import, or extension implementation is shipped.
- Android and Linux builds pass automated tests, accessibility checks, packaging smoke tests, and a documented security review.
