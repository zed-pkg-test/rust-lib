//! The one thing this zed-sourced crate exports.
pub fn greet(who: &str) -> String {
    format!("hello {who} from zed-pkg-test/rust-lib")
}

#[cfg(test)]
mod tests {
    use super::greet;

    #[test]
    fn greet_identifies_the_immutable_zed_package_namespace() {
        assert_eq!(
            greet("consumer"),
            "hello consumer from zed-pkg-test/rust-lib"
        );
    }
}
