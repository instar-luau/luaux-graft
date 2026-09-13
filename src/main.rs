//! Native `LuauX` compilation graft for Instar.

use luaux::{Config, Element, Table, compile::compile_configured, config::BackendKind};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    error::Error,
    io::{self, Read, Write},
    process::ExitCode,
};

const PAYLOAD_LIMIT: u64 = 64 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u32,
    hook: String,
    source: String,
    configuration: BTreeMap<String, serde_json::Value>,
    settings: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct Mapping {
    start: usize,
    end: usize,
    original_start: usize,
    original_end: usize,
}

#[derive(Serialize)]
struct Compilation {
    version: u32,
    source: String,
    dependencies: Vec<String>,
    mappings: Vec<Mapping>,
}

fn mappings(source: &str, output: &str) -> io::Result<Vec<Mapping>> {
    let mut originals = source.split_inclusive('\n');
    let mut generated = output.split_inclusive('\n');
    let mut original_start = 0;
    let mut start = 0;
    let mut mappings = Vec::new();

    loop {
        match (originals.next(), generated.next()) {
            (Some(original), Some(generated)) => {
                let end = start + generated.len();

                mappings.push(Mapping {
                    start,
                    end,
                    original_start,
                    original_end: if original == generated {
                        original_start + original.len()
                    } else {
                        original_start
                    },
                });

                start = end;
                original_start += original.len();
            }

            (None, None) => return Ok(mappings),
            _ => return Err(io::Error::other("LuauX changed the source line count")),
        }
    }
}

fn diagnostic(source: &str, offset: usize, message: &str, help: Option<&str>) -> String {
    let prefix = source.get(..offset).unwrap_or_default();
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix.rsplit('\n').next().unwrap_or_default().len() + 1;
    let mut message = format!("({line},{column}): {message}");

    if let Some(help) = help {
        message.push_str("; ");
        message.push_str(help);
    }

    message
}

fn compile(request: &Request) -> Result<Compilation, Box<dyn Error>> {
    if request.version != 1 || request.hook != "compile" || request.settings.is_some() {
        return Err("expected a protocol 1 compile request without formatter settings".into());
    }

    let (configuration, warnings) =
        Config::parse_reporting(&toml::to_string(&request.configuration)?)?;

    for warning in warnings {
        eprintln!("warning: {warning}");
    }

    let backend: &dyn luaux::Backend = match configuration.backend {
        BackendKind::Element => &Element,
        BackendKind::Table => &Table,
    };

    let (output, warnings) =
        compile_configured(&request.source, backend, configuration).map_err(|error| {
            io::Error::other(diagnostic(
                &request.source,
                error.offset,
                &error.message,
                error.help.as_deref(),
            ))
        })?;

    for warning in warnings {
        eprintln!(
            "warning: {}",
            diagnostic(
                &request.source,
                warning.offset,
                &warning.message,
                warning.help.as_deref()
            )
        );
    }

    let mappings = mappings(&request.source, &output)?;

    Ok(Compilation {
        version: 1,
        source: output,
        dependencies: Vec::new(),
        mappings,
    })
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut input = Vec::new();

    io::stdin()
        .lock()
        .take(PAYLOAD_LIMIT + 1)
        .read_to_end(&mut input)?;

    if input.len() as u64 > PAYLOAD_LIMIT {
        return Err("graft request exceeds the payload limit".into());
    }

    let request = serde_json::from_slice(&input)?;
    let output = serde_json::to_vec(&compile(&request)?)?;

    if output.len() as u64 > PAYLOAD_LIMIT {
        return Err("graft response exceeds the payload limit".into());
    }

    io::stdout().lock().write_all(&output)?;

    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,

        Err(error) => {
            eprintln!("luaux-graft: {error}");

            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mappings_preserve_unchanged_bytes_and_anchor_transformed_lines() {
        let source = "-- 雪\r\nreturn <Frame />\n";
        let output = "-- 雪\r\nreturn React.createElement(\"Frame\")\n";
        let mappings = mappings(source, output).unwrap();
        assert_eq!(mappings.len(), 2);
        assert_eq!(mappings[0].original_end, "-- 雪\r\n".len());
        assert_eq!(mappings[1].original_start, mappings[1].original_end);
        assert_eq!(mappings[1].end, output.len());
        assert!(self::mappings("", "").unwrap().is_empty());
        assert!(self::mappings("return 1", "return 1\nreturn 2").is_err());
    }
}
