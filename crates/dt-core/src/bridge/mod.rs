//! Ways to get console commands into a running game.

pub mod ack;
pub mod boot;
pub mod clipboard;
pub mod conlog;
pub mod execfile;
pub mod netcon;

pub use ack::Receipt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleCmd {
    pub name: String,
    pub value: String,
}

impl ConsoleCmd {
    /// `name "value"`, quoted so values with spaces survive.
    ///
    /// The Source console has no escape for `"` inside a quoted token, and `;` or a
    /// line break starts a second command, so such values are refused rather than escaped.
    pub fn to_line(&self) -> Result<String, BridgeError> {
        let refuse = |reason: &'static str| BridgeError::Unsafe {
            name: self.name.clone(),
            reason,
        };
        let name_ok = !self.name.is_empty()
            && self
                .name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !name_ok {
            return Err(refuse("name must be ASCII letters, digits or _"));
        }
        if self
            .value
            .chars()
            .any(|c| matches!(c, '"' | ';' | '\n' | '\r'))
        {
            return Err(refuse("value contains a quote, semicolon or line break"));
        }
        Ok(format!("{} \"{}\"", self.name, self.value))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("convar {0} is on the denylist")]
    Denied(String),
    #[error("refusing to send {name}: {reason}")]
    Unsafe { name: String, reason: &'static str },
}

pub trait Bridge {
    fn name(&self) -> &'static str;
    /// Delivers console lines to the game, in order, as one unit.
    fn send(&mut self, lines: &[String]) -> Result<(), BridgeError>;

    /// The batch plus the ack trailer `ack::script` adds, so the console log can confirm it.
    fn push(&mut self, cmds: &[ConsoleCmd]) -> Result<Receipt, BridgeError> {
        let receipt = Receipt::for_batch(cmds);
        self.send(&ack::script(&receipt)?)?;
        Ok(receipt)
    }

    /// A batch that changes nothing and only queries `name`, to prove the pipeline works.
    fn probe(&mut self, name: &str) -> Result<Receipt, BridgeError> {
        let receipt = Receipt::for_probe(name);
        self.send(&ack::script(&receipt)?)?;
        Ok(receipt)
    }
}

/// All lines or the first error, so a bridge never sends half a batch.
fn lines(cmds: &[ConsoleCmd]) -> Result<Vec<String>, BridgeError> {
    cmds.iter().map(ConsoleCmd::to_line).collect()
}

#[cfg(test)]
fn cmd(name: &str, value: &str) -> ConsoleCmd {
    ConsoleCmd {
        name: name.into(),
        value: value.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_line_quotes_value() {
        assert_eq!(cmd("fps_max", "240").to_line().unwrap(), r#"fps_max "240""#);
        assert_eq!(
            cmd("r_name", "a b").to_line().unwrap(),
            r#"r_name "a b""#,
            "spaces stay inside one quoted token"
        );
        assert_eq!(cmd("r_x", "").to_line().unwrap(), r#"r_x """#);
    }

    #[test]
    fn to_line_refuses_values_that_could_inject_a_second_command() {
        for value in [r#"1"; quit; echo ""#, "1; quit", "1\nquit", "1\rquit", "\""] {
            assert!(
                matches!(
                    cmd("fps_max", value).to_line(),
                    Err(BridgeError::Unsafe { .. })
                ),
                "value {value:?} must be refused"
            );
        }
    }

    #[test]
    fn to_line_refuses_names_that_are_not_identifiers() {
        for name in ["", "quit;fps_max", "fps max", "a\"b", "bind\n"] {
            assert!(
                matches!(cmd(name, "1").to_line(), Err(BridgeError::Unsafe { .. })),
                "name {name:?} must be refused"
            );
        }
    }
}
