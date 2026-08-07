# rust-lib (`zed-pkg-test/rust-lib`)

A real Rust crate published through the Zed package registry while retaining
ordinary Cargo semantics.

## Interoperability contract

- Zed owns distribution and materializes the crate under the consumer's
  `[install].dir`, normally `.vendor/.zed/zed-pkg-test/rust-lib`.
- Cargo consumes that materialization as a normal `path` dependency.
- `Cargo.toml` owns crates.io dependencies and Cargo features.
- `.zpkg.toml` owns Zed package identity and Zed-to-Zed dependencies; crates.io
  packages are deliberately not repeated there.
- Library source does not track `Cargo.lock`, matching standard Cargo library
  practice. Binary consumers do track their own lockfile.

Version `1.0.1` depends on the exact crates.io package `itoa = 1.0.18`. The
`format_count` API proves that dependency is present after Zed publication and
installation. The optional `uppercase` feature is used by the companion
`zed-pkg-test/rust-app` fixture to prove Cargo resolver-v2 feature separation
between target and build dependencies.

A consumer uses both package managers explicitly:

```toml
# Cargo.toml
[dependencies]
rust-lib = {
  path = ".vendor/.zed/zed-pkg-test/rust-lib",
  features = ["uppercase"]
}
```

```toml
# .zpkg.toml
[install]
dir = ".vendor/.zed"

[dependencies]
"zed-pkg-test/rust-lib" = "^1.0.0"
```

The Zed publish smoke test builds a separate Cargo application with this path
dependency plus an independent crates.io dependency, locks it, runs it, and
replays it offline without creating a Zed lockfile in the Cargo-only consumer.
