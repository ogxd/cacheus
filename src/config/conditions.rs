use serde::{Serialize, Deserialize};
use crate::buffered_body::BufferedBody;
use hyper::Request;

/// Conditions for evaluating requests and responses
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum OnRequestCondition {
    HeaderExists { header_exists: String },
    PathContains { path_contains: String },
    StatusCodeEquals { status_code: u16 },
    All { all: Vec<OnRequestCondition> },
    Any { any: Vec<OnRequestCondition> },
    Not { not: Box<OnRequestCondition> },
}

impl OnRequestCondition {
    /// Evaluate the condition against a request
    pub fn evaluate(&self, req: &Request<BufferedBody>) -> bool {
        match self {
            OnRequestCondition::HeaderExists { header_exists } => req.headers().contains_key(header_exists),
            OnRequestCondition::PathContains { path_contains } => req.uri().path_and_query().unwrap().as_str().contains(path_contains),
            OnRequestCondition::All { all } => all.iter().all(|c| c.evaluate(req)),
            OnRequestCondition::Any { any } => any.iter().any(|c| c.evaluate(req)),
            OnRequestCondition::Not { not } => !not.evaluate(req),
            _ => false, // Ignore response conditions at this phase
        }
    }

    // TODO: Add evaluate_response method when needed
    // pub fn evaluate_response(&self, req: &Request<BufferedBody>, res: &Response<BufferedBody>) -> bool {
    //     match self {
    //         OnRequestCondition::StatusCodeEquals { status_code } => res.status().as_u16().eq(status_code),
    //         OnRequestCondition::All { all } => all.iter().all(|c| c.evaluate_response(req, res)),
    //         OnRequestCondition::Any { any } => any.iter().any(|c| c.evaluate_response(req, res)),
    //         _ => false,
    //     }
    // }
}
