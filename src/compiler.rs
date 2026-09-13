use crate::protocol::{Compilation, Mapping, Request};
use luaux::{Backend, Config, Element, Table, compile::compile_configured, config::BackendKind};
use std::{collections::BTreeMap, error::Error, io};

pub(crate) fn compile(request: &Request) -> Result<Compilation, Box<dyn Error>> {
    if request.version != 1 || request.hook != "compile" || request.settings.is_some() {
        return Err("expected a protocol 1 compile request without formatter settings".into());
    }

    let configuration = configuration(&request.configuration)?;
    let backend = backend(&configuration);

    let (source, warnings) = compile_configured(&request.source, backend.as_ref(), configuration)
        .map_err(|error| {
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
                warning.help.as_deref(),
            )
        );
    }

    Ok(Compilation {
        version: 1,
        mappings: mappings(&request.source, &source)?,
        source,
        dependencies: Vec::new(),
    })
}

pub(crate) fn configuration(
    values: &BTreeMap<String, serde_json::Value>,
) -> Result<Config, Box<dyn Error>> {
    let (configuration, warnings) = Config::parse_reporting(&toml::to_string(values)?)?;

    for warning in warnings {
        eprintln!("warning: {warning}");
    }

    Ok(configuration)
}

pub(crate) fn backend(configuration: &Config) -> Box<dyn Backend> {
    match configuration.backend {
        BackendKind::Element => Box::new(Element),
        BackendKind::Table => Box::new(Table),
    }
}

pub(crate) fn mappings(source: &str, generated: &str) -> io::Result<Vec<Mapping>> {
    let mut original_lines = source.split_inclusive('\n');
    let mut generated_lines = generated.split_inclusive('\n');
    let mut original_start = 0;
    let mut generated_start = 0;
    let mut mappings = Vec::new();

    loop {
        match (original_lines.next(), generated_lines.next()) {
            (Some(original), Some(generated)) => {
                let generated_end = generated_start + generated.len();

                mappings.push(Mapping {
                    start: generated_start,
                    end: generated_end,
                    original_start,
                    original_end: original_start + original.len(),
                });

                generated_start = generated_end;
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

    match help {
        Some(help) => format!("({line},{column}): {message}; {help}"),
        None => format!("({line},{column}): {message}"),
    }
}
