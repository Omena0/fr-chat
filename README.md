# fr-chat

fr-chat is a Matrix client for Android and Linux, built with a Rust application core and Compose Multiplatform UI. The interface uses familiar server, channel, and direct-message semantics. The current desktop window is an early disconnected shell; Matrix sign-in and live conversations are not wired yet.

## Run on Linux

Requirements: JDK 21 and network access for the first Gradle dependency download.

```sh
./gradlew :app:linux:run
```

## Build Android

Requires Android SDK API 36 and Build-Tools 36 with the Android SDK licenses accepted.

```sh
./gradlew :app:android:assembleDebug
```

## Test the Rust core

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## Layout

- `app/shared`: Compose Multiplatform UI shared by Android and Linux.
- `app/linux`: Linux Desktop launcher and packaging.
- `app/android`: Android application entry point.
- `crates/domain`: provider-neutral product models and invariants.
- `crates/provider-api`: account, sync, action, and capability contracts.
- `crates/app-core`: provider orchestration and application use cases.
- `crates/store-sqlite`: encrypted SQLite persistence and outbox.
