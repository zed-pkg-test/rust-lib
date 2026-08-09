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
publish = false

[dependencies]
rust-lib = { path = "$ZED_PKG_TEST_TARGET", features = ["uppercase"] }
unicode-ident = "=1.0.24"
EOF

cat >"$smoke_root/src/main.rs" <<'EOF'
fn main() {
    assert_eq!(
        rust_lib::greet("r2g"),
        "HELLO R2G FROM ZED-PKG-TEST/RUST-LIB"
    );
    assert_eq!(rust_lib::format_count(1_000_001), "1000001");
    assert!(unicode_ident::is_xid_start('Δ'));
}
EOF

cd "$smoke_root"
CARGO_TARGET_DIR="$smoke_root/target" cargo generate-lockfile
cp Cargo.lock Cargo.lock.expected
CARGO_TARGET_DIR="$smoke_root/target" cargo metadata \
  --locked \
  --format-version 1 >metadata.json

grep -Fq '"name":"rust-lib"' metadata.json
grep -Fq '"name":"itoa"' metadata.json
grep -Fq '"name":"unicode-ident"' metadata.json
grep -Fq 'registry+https://github.com/rust-lang/crates.io-index' Cargo.lock

test ! -e .zpkg.lock
CARGO_TARGET_DIR="$smoke_root/target" cargo run --locked --quiet
CARGO_TARGET_DIR="$smoke_root/target" cargo run --locked --offline --quiet
cmp Cargo.lock.expected Cargo.lock
test ! -e .zpkg.lock
