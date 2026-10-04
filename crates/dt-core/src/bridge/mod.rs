//! Ways to get console commands into a running game.

pub mod clipboard;
pub mod execfile;
pub mod netcon;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleCmd {
    pub name: String,
    pub value: String,
}

impl ConsoleCmd {
    /// `name "value"`, quoted so values with spaces survive.
    pub fn to_line(&self) -> String {
        todo!()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("convar {0} is on the denylist")]
    Denied(String),
}

pub trait Bridge {
    fn name(&self) -> &'static str;
    fn push(&mut self, cmds: &[ConsoleCmd]) -> Result<(), BridgeError>;
}
