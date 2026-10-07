# User Experience

## Information Architecture

- Use Discord-familiar terms and interaction semantics as the default product model across the entire app: **servers**, **channels**, **direct messages**, **threads**, **members**, and **roles** where supported. Map those concepts to Matrix spaces, rooms, users, and power levels internally; provider-specific details remain available in channel/account menus rather than changing the app's primary vocabulary.
- Keep a persistent community/server rail on wide desktop layouts, a channel/navigation pane beside the active conversation, and a conversation-focused main pane. Support a member pane where room size and viewport permit.
- On compact widths, use one primary pane at a time with clear back navigation and preserved scroll/read position. Do not compress the desktop three-pane layout into unusable narrow columns.
- On Android, support system back, edge-to-edge insets, keyboard/IME resizing, selection and share intents, and accessible touch targets.
- On Linux, support resizable panes, keyboard navigation, shortcuts, system file selection, desktop notifications, and deep links.
- Use a distinct product palette, typography, iconography, and motion. Reuse interaction conventions, not Discord's assets, exact branding, or copied screens.
- Do not add a permanent origin badge to every channel or message. A channel submenu/details view must identify the backing provider (Matrix or an extension/bridge) and expose supported capabilities. Explain non-obvious limitations at the point of action.
- Render bridged users with their bridge-provided display name/avatar where available. Preserve the actual Matrix sender identity and remote identity mapping in message details; do not silently merge identities.
- Respect Matrix room power levels and provider capabilities. Hide unavailable actions or explain why they cannot be performed rather than presenting controls that fail silently.
- Provide light and dark themes, configurable text size, reduced-motion behavior, screen-reader labels, focus visibility, and WCAG 2.2 AA-oriented contrast and keyboard support.
