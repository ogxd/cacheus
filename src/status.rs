use std::fmt::{Display, Formatter, Result};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Status {
    Hit,
    Miss,
    Bypass,
    Reject,
}

impl Display for Status {
    fn fmt(&self, f: &mut Formatter) -> Result {
        match self {
            Status::Hit => write!(f, "hit"),
            Status::Miss => write!(f, "miss"),
            Status::Bypass => write!(f, "bypass"),
            Status::Reject => write!(f, "reject")
        }
    }
}