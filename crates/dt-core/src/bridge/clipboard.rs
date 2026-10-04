//! The UI owns the actual clipboard; core only builds the paste string.

use super::{BridgeError, ConsoleCmd};

/// `a "1"; b "2"`, ready to paste into the console.
pub fn batch_string(cmds: &[ConsoleCmd]) -> Result<String, BridgeError> {
    Ok(super::lines(cmds)?.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::cmd;

    #[test]
    fn joins_with_semicolon_space() {
        let s = batch_string(&[cmd("a", "1"), cmd("b", "2 3")]).unwrap();
        assert_eq!(s, r#"a "1"; b "2 3""#);
    }

    #[test]
    fn empty_batch_is_empty_string() {
        assert_eq!(batch_string(&[]).unwrap(), "");
    }

    #[test]
    fn one_unsafe_command_fails_the_whole_batch() {
        assert!(batch_string(&[cmd("a", "1"), cmd("b", "1; quit")]).is_err());
    }
}
