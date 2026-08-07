# Cargo interoperability contract

This fixture proves that Zed package distribution and Cargo dependency
resolution remain separate, composable layers.

## Ownership

- `.zpkg.toml` identifies the Zed package and any Zed package dependencies.
- `Cargo.toml` identifies crates.io, Git, and Cargo path dependencies plus Cargo
  features.
- library source does not track `Cargo.lock`;
- a consuming binary owns its exact Cargo lock;
- `.zpkg.lock` and `Cargo.lock` may coexist and neither package manager rewrites
  the other's lockfile.

## Certified scenario

`rust-lib` is published and installed through Zed. Its Cargo manifest depends on
`itoa = 1.0.18`. The publish smoke test then creates an external binary that:

1. points Cargo at the installed Zed package using a path dependency;
2. enables the `uppercase` Cargo feature;
3. declares its own independent `unicode-ident = 1.0.24` crates.io dependency;
4. generates and retains a Cargo lock;
5. verifies Cargo metadata contains one path package and two registry packages;
6. runs with `--locked`; and
7. replays with `--locked --offline` without creating `.zpkg.lock`.

The companion `rust-app` fixture extends this with normal/build dependency
feature separation, Zed symlink/copy modes, dual-lock immutability, and
cross-platform Cargo execution.
