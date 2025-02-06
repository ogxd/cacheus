use std::fmt::{Display, Formatter, Result};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Status {
    Hit,
    HitShort,
    Miss,
    Reject,
}

impl Display for Status {
    fn fmt(&self, f: &mut Formatter) -> Result {
        match self {
            Status::Hit => write!(f, "hit"),
            Status::HitShort => write!(f, "hit-short"),
            Status::Miss => write!(f, "miss"),
            Status::Reject => write!(f, "reject")
        }
    }
}