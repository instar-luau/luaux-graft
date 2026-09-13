use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub(crate) version: u32,
    pub(crate) hook: String,
    pub(crate) source: String,
    pub(crate) configuration: BTreeMap<String, serde_json::Value>,
    pub(crate) settings: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub(crate) struct Compilation {
    pub(crate) version: u32,
    pub(crate) source: String,
    pub(crate) dependencies: Vec<String>,
    pub(crate) mappings: Vec<Mapping>,
}

#[derive(Serialize)]
pub(crate) struct Mapping {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) original_start: usize,
    pub(crate) original_end: usize,
}

#[derive(Serialize)]
pub(crate) struct Format {
    pub(crate) version: u32,
    pub(crate) document: Document,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Document {
    #[serde(rename = "empty")]
    Nil,

    Source(u32, u32),

    #[serde(rename = "text")]
    Literal(String),

    Line,
    Soft,
    Hard,
    Blank,
    Group(Box<Self>),
    Indent(Box<Self>),

    #[serde(rename = "sequence")]
    Concatenate(Vec<Self>),

    Host {
        start: u32,
        end: u32,
        parse: ParseMode,
    },
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ParseMode {
    Block,
    Expression,
}

impl Document {
    pub(crate) fn source(start: usize, end: usize) -> Self {
        Self::Source(offset(start), offset(end))
    }

    pub(crate) fn literal(text: impl Into<String>) -> Self {
        Self::Literal(text.into())
    }

    pub(crate) fn group(inner: Self) -> Self {
        Self::Group(Box::new(inner))
    }

    pub(crate) fn indent(inner: Self) -> Self {
        Self::Indent(Box::new(inner))
    }

    pub(crate) fn concatenate(parts: impl IntoIterator<Item = Self>) -> Self {
        Self::Concatenate(parts.into_iter().collect())
    }

    pub(crate) fn host(start: usize, end: usize) -> Self {
        Self::Host {
            start: offset(start),
            end: offset(end),
            parse: ParseMode::Block,
        }
    }

    pub(crate) fn expression(start: usize, end: usize) -> Self {
        Self::Host {
            start: offset(start),
            end: offset(end),
            parse: ParseMode::Expression,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct Finding {
    pub(crate) rule: String,
    pub(crate) message: String,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

pub(crate) fn offset(value: usize) -> u32 {
    u32::try_from(value).expect("source exceeds the graft offset limit")
}
