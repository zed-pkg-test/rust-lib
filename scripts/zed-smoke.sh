#!/usr/bin/env sh
set -eu

: "${ZED_PKG_TEST_TARGET:?ZED_PKG_TEST_TARGET is required}"
smoke_root="$(mktemp -d "${TMPDIR:-/tmp}/zed-pkg-rust-smoke.XXXXXXXX")"
trap 'rm -rf "$smoke_root"' EXIT HUP INT TERM
mkdir -p "$smoke_root/src"

cat >"$smoke_root/Cargo.toml" <<EOF
[package]
name = "zed-pkg-rust-smoke"
version = "0.0.0"
edition = "2021"

[dependencies]
rust-lib = { path = "$ZED_PKG_TEST_TARGET" }
EOF

cat >"$smoke_root/src/main.rs" <<'EOF'
fn main() {
    assert_eq!(
        rust_lib::greet("r2g"),
        "hello r2g from zed-pkg-test/rust-lib"
    );
}
EOF

CARGO_TARGET_DIR="$smoke_root/target" cargo run \
  --quiet \
  --manifest-path "$smoke_root/Cargo.toml"
