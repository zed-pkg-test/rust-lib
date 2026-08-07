#[test]
fn cargo_feature_and_registry_dependency_are_publicly_usable() {
    let expected = if cfg!(feature = "uppercase") {
        "HELLO INTEGRATION FROM ZED-PKG-TEST/RUST-LIB"
    } else {
        "hello integration from zed-pkg-test/rust-lib"
    };
    assert_eq!(rust_lib::greet("integration"), expected);
    assert_eq!(rust_lib::format_count(4_294_967_296), "4294967296");
}
