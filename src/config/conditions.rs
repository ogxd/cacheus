use serde::{Serialize, Deserialize};
use crate::buffered_body::BufferedBody;
use hyper::{Request, Response};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Condition {
    HeaderExists { header_exists: String },
    PathContains { path_contains: String },
    StatusCodeIs { status_code_is: u16 },
    All { all: Vec<Condition> },
    Any { any: Vec<Condition> },
    Not { not: Box<Condition> },
}

impl Condition {
    pub fn evaluate(&self, request: &Request<BufferedBody>) -> bool {
        match self {
            Condition::HeaderExists { header_exists } => request.headers().contains_key(header_exists),
            Condition::PathContains { path_contains } => contains_wildcard(&request.uri().path_and_query().unwrap().as_str(), path_contains),
            Condition::All { all } => all.iter().all(|c| c.evaluate(request)),
            Condition::Any { any } => any.iter().any(|c| c.evaluate(request)),
            Condition::Not { not } => !not.evaluate(request),
            _ => true,
        }
    }

    pub fn evaluate_with_response(&self, request: &Request<BufferedBody>, response: &Option<Response<BufferedBody>>) -> bool {
        match self {
            Condition::HeaderExists { header_exists } => request.headers().contains_key(header_exists), // Response headers to consider?
            Condition::PathContains { path_contains } => contains_wildcard(&request.uri().path_and_query().unwrap().as_str(), path_contains),
            Condition::StatusCodeIs { status_code_is } => response.as_ref().map_or(false, |resp| resp.status().as_u16() == *status_code_is),
            Condition::All { all } => all.iter().all(|c| c.evaluate_with_response(request, response)),
            Condition::Any { any } => any.iter().any(|c| c.evaluate_with_response(request, response)),
            Condition::Not { not } => !not.evaluate_with_response(request, response),
        }
    }
}

fn contains_wildcard(text: &str, pattern: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();

    if parts.len() == 1 {
        return text == pattern;
    }

    let starts_with = if !pattern.starts_with('*') {
        if let Some(first) = parts.first() {
            if !text.starts_with(first) {
                return false;
            }
            Some(first.len())
        } else {
            None
        }
    } else {
        Some(0)
    };

    if !pattern.ends_with('*') {
        if let Some(last) = parts.last() {
            if !text.ends_with(last) {
                return false;
            }
        }
    };

    let mut pos = starts_with.unwrap_or(0);
    for part in parts.iter().filter(|&&p| !p.is_empty()).skip(1).take(parts.len() - 2) {
        if let Some(found) = text[pos..].find(part) {
            pos += found + part.len();
        } else {
            return false;
        }
    }

    true
}


#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn contains_wildcard_test()
    {
        assert!(contains_wildcard("hello world!", "*hello*"));
        assert!(!contains_wildcard("hello world!", "*hello"));
        assert!(contains_wildcard("abcde", "a*e"));
        assert!(!contains_wildcard("abcdf", "a*e"));
        assert!(contains_wildcard("abc", "abc"));
        assert!(contains_wildcard("abc", "*abc*"));
    }
}