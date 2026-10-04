//! The UI owns the actual clipboard; core only builds the paste string.

use super::ConsoleCmd;

/// `a "1"; b "2"`, ready to paste into the console.
pub fn batch_string(cmds: &[ConsoleCmd]) -> String {
    let _ = cmds;
    todo!()
}
