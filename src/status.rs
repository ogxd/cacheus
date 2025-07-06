use std::fmt::{Display, Formatter, Result};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Status {
    Hit,
    Miss,
    Block,
    Ignore,
}

impl Display for Status {
    fn fmt(&self, f: &mut Formatter) -> Result {
        match self {
            Status::Hit => write!(f, "hit"),
            Status::Miss => write!(f, "miss"),
            Status::Block => write!(f, "block"),
            Status::Ignore => write!(f, "ignore")
        }
    }
}