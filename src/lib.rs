//! The one thing this zed-sourced crate exports.
pub fn greet(who: &str) -> String {
    format!("hello {who} from zedtest/rust-lib")
}
