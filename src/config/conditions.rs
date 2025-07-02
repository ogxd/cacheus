use serde::{Serialize, Deserialize};
use crate::buffered_body::BufferedBody;
use hyper::{Request, Response};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum OnRequestCondition {
    HeaderExists { header_exists: String },
    PathContains { path_contains: String },
    All { all: Vec<OnRequestCondition> },
    Any { any: Vec<OnRequestCondition> },
    Not { not: Box<OnRequestCondition> },
}

impl OnRequestCondition {
    pub fn evaluate(&self, request: &Request<BufferedBody>) -> bool {
        match self {
            OnRequestCondition::HeaderExists { header_exists } => request.headers().contains_key(header_exists),
            OnRequestCondition::PathContains { path_contains } => request.uri().path_and_query().unwrap().as_str().contains(path_contains),
            OnRequestCondition::All { all } => all.iter().all(|c| c.evaluate(request)),
            OnRequestCondition::Any { any } => any.iter().any(|c| c.evaluate(request)),
            OnRequestCondition::Not { not } => !not.evaluate(request),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum OnResponseCondition {
    HeaderExists { header_exists: String },
    StatusCodeIs { status_code_is: u16 },
    All { all: Vec<OnResponseCondition> },
    Any { any: Vec<OnResponseCondition> },
    Not { not: Box<OnResponseCondition> },
}

impl OnResponseCondition {
    pub fn evaluate(&self, request: &Request<BufferedBody>, response: &Response<BufferedBody>) -> bool {
        match self {
            OnResponseCondition::HeaderExists { header_exists } => request.headers().contains_key(header_exists),
            OnResponseCondition::StatusCodeIs { status_code_is } => response.status().as_u16() == *status_code_is,
            OnResponseCondition::All { all } => all.iter().all(|c| c.evaluate(request, response)),
            OnResponseCondition::Any { any } => any.iter().any(|c| c.evaluate(request, response)),
            OnResponseCondition::Not { not } => !not.evaluate(request, response),
        }
    }
}
