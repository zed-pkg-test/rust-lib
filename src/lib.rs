//! Cargo crate fixture distributed through the Zed package registry.

/// Return a greeting that identifies the immutable Zed package namespace.
///
/// The optional `uppercase` Cargo feature intentionally changes only the
/// consumer-facing target build. Cargo resolver v2 can therefore prove that a
/// normal dependency and a build dependency may use separate feature sets even
/// when both point at the same Zed-materialized crate.
pub fn greet(who: &str) -> String {
    let message = format!("hello {who} from zed-pkg-test/rust-lib");
    #[cfg(feature = "uppercase")]
    let message = message.to_uppercase();
    message
}

/// Format an integer through a real crates.io dependency.
///
/// This keeps the fixture small while proving that a crate installed by Zed can
/// retain ordinary Cargo registry dependencies without duplicating them in
/// `.zpkg.toml`.
pub fn format_count(value: u64) -> String {
    let mut buffer = itoa::Buffer::new();
    buffer.format(value).to_owned()
}

#[cfg(test)]
mod tests {
    use super::{format_count, greet};

    #[test]
    fn greet_identifies_the_immutable_zed_package_namespace() {
        let expected = if cfg!(feature = "uppercase") {
            "HELLO CONSUMER FROM ZED-PKG-TEST/RUST-LIB"
        } else {
            "hello consumer from zed-pkg-test/rust-lib"
        };
        assert_eq!(greet("consumer"), expected);
    }

    #[test]
    fn cargo_registry_dependency_is_exercised() {
        assert_eq!(format_count(18_446_744_073_709_551_615), "18446744073709551615");
    }
}
