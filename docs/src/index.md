# fr-chat Documentation

Product, architecture, and delivery documentation for the Android and Linux Matrix client.

The app uses Discord-familiar server and channel semantics throughout its interface, backed initially by Matrix. It is built around a Rust application core and a Compose Multiplatform UI. Platform extensions are described by a platform-neutral API, but extension implementations are not part of the current scope.

## Documentation

- [Product Overview](product.md): goals, audience, and primary journeys.
- [User Experience](experience.md): information architecture, responsive behavior, and interaction principles.
- [Functional Requirements](requirements.md): Matrix, mautrix, extension API, and privacy requirements.
- [Architecture](architecture.md): Rust core, Compose UI, bindings, and module boundaries.
- [Core Contract](core-contract.md): model ownership, provider/storage ports, sync durability, and implementation gates.
- [Security and Acceptance](quality.md): security requirements, quality gates, and release criteria.
- [Implementation Plan](plan.md): phased roadmap and unresolved decisions.
