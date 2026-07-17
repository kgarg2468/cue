# Capture → Delegate

This repository currently contains the bounded M0 Foundation slice: a native
SwiftUI macOS client package and a separate Rust backend process that complete
a versioned Unix-domain-socket health handshake. It is not the completed
product or an MVP.

The Swift health executable sends `{"version":1,"type":"health"}` and the Rust
service replies with `{"version":1,"type":"health_response","status":"ok"}`.

Prerequisites: macOS 14+, Swift 6.0, and Rust/Cargo.

Run the full local verification suite with:

```sh
scripts/verify-m0.sh
```

For local native-app runtime evidence, build the release executable and run:

```sh
swift build -c release --disable-sandbox
scripts/verify-app-launch.sh
```

The launch smoke verifies through System Events that the app is a visible,
non-background process with a native window, remains alive briefly, and then
terminates it. It is intentionally not part of `verify-m0.sh`: GitHub's macOS
runners do not provide a reliable interactive WindowServer/System Events UI
session, so this check would be flaky or fail headlessly in CI.

The authoritative full-product specification is
`.context/attachments/2UKdwq/capture_delegate_macos_product_ux_spec.md`.
