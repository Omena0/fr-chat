# Product Overview

## Product Summary

fr-chat is a production-quality Matrix client for Android and Linux desktop, with a Rust application/protocol core and a shared Compose Multiplatform UI. The app uses Discord's familiar interaction semantics by default for every provider: servers, channels, direct messages, threads, members, and roles where supported. These are product-level UI concepts, not a migration mode or a Discord-only skin. The app must have its own visual identity and must not use Discord branding or assets.

The initial release is a Matrix client, not a Discord migration tool or a platform-extension host. It must handle mautrix-bridged rooms and users especially well. The product includes a platform-agnostic extension API design for future providers, plus a design and feasibility plan for a Discord extension; implementing or shipping any extension is out of scope. The API is intended to represent user-authorized account actions where the provider permits them, and does not support bot accounts.

## Product Goals

- Make Matrix communities approachable to people accustomed to Discord terminology and interaction patterns.
- Provide a complete, reliable Matrix messaging experience on Android and Linux.
- Present bridged conversations and ghost users as natural parts of the client, while making their origin and capabilities discoverable on demand.
- Define an extension contract that can represent platform accounts, conversations, events, and user-authorized actions without coupling the common UI or domain model to a specific platform.
- Keep platform-specific lifecycle, storage, notifications, and integrations behind narrow platform interfaces so additional operating systems can be added later.
- Treat security, privacy, accessibility, responsiveness, offline behavior, and testing as product requirements rather than later polish.

## Non-Goals for Initial Release

- Importing, copying, or migrating Discord servers, messages, accounts, or history into Matrix.
- Implementing or shipping platform extensions, including a Discord connector.
- Supporting bot accounts as provider accounts.
- Automating undocumented/private endpoints, bypassing platform restrictions, or impersonating an identity the user does not control or is not authorized to use.
- Reimplementing Matrix homeserver, federation, or bridge infrastructure inside the client.
- Requiring a particular homeserver, bridge operator, or hosting provider.
- Making every remote platform feature appear supported when the underlying provider does not expose it.

## Users and Primary Journeys

### Target users

- People adopting Matrix who want familiar group-chat navigation.
- Existing Matrix users who participate in communities connected through mautrix bridges.
- Community members who use both desktop and Android and expect their rooms, read state, and preferences to remain coherent.
- Future extension authors integrating another platform through an authorized provider implementation.

### Core journeys

1. Sign in to a Matrix account, verify the device, and land in a familiar server/channel layout.
2. Browse spaces and rooms, join a room, search, and participate in public or encrypted conversations.
3. Open direct messages, reply in threads, react, edit or redact a message, share media, and navigate unread activity.
4. Participate in a mautrix-bridged room without needing to understand bridge internals; inspect its platform origin and available actions from the channel submenu.
5. Use the same account across Linux and Android, with platform-native notifications, secure credential storage, and responsive layouts.
6. In a future release, use an extension built against the platform-neutral API to authorize a supported user account and view its conversations through the same server/channel experience.
