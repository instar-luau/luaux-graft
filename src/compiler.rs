use crate::protocol::{Compilation, Mapping, Request};
use luaux::{Backend, Config, Element, Table, compile::compile_configured, config::BackendKind};
use std::{collections::BTreeMap, error::Error, io};

pub(crate) fn compile(request: &Request) -> Result<Compilation, Box<dyn Error>> {
    if request.version != 1 || request.hook != "compile" || request.settings.is_some() {
        return Err("expected a protocol 1 compile request without formatter settings".into());
    }

    let configuration = configuration(request)?;
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

pub(crate) fn configuration(request: &Request) -> Result<Config, Box<dyn Error>> {
    let mut values = request
        .path
        .as_deref()
        .and_then(std::path::Path::parent)
        .and_then(|directory| {
            directory
                .ancestors()
                .map(|ancestor| ancestor.join("luaux.toml"))
                .find(|path| path.is_file())
        })
        .map(std::fs::read_to_string)
        .transpose()?
        .map(|source| toml::from_str::<BTreeMap<String, serde_json::Value>>(&source))
        .transpose()?
        .unwrap_or_default();

    overlay(&mut values, request.configuration.clone());

    let (configuration, warnings) = Config::parse_reporting(&toml::to_string(&values)?)?;

    for warning in warnings {
        eprintln!("warning: {warning}");
    }

    Ok(configuration)
}

fn overlay(
    base: &mut BTreeMap<String, serde_json::Value>,
    overlay: BTreeMap<String, serde_json::Value>,
) {
    for (name, value) in overlay {
        match (base.get_mut(&name), value) {
            (Some(serde_json::Value::Object(base)), serde_json::Value::Object(overlay)) => {
                for (name, value) in overlay {
                    base.insert(name, value);
                }
            }

            (_, value) => {
                base.insert(name, value);
            }
        }
    }
}

pub(crate) fn backend(configuration: &Config) -> Box<dyn Backend> {
    match configuration.backend {
        BackendKind::Element => Box::new(Element),
        BackendKind::Table => Box::new(Table),
    }
}

pub(crate) fn mappings(source: &str, generated: &str) -> io::Result<Vec<Mapping>> {
    let original_lines = lines(source);
    let generated_lines = lines(generated);

    if original_lines.len() != generated_lines.len() {
        return Err(io::Error::other("LuauX changed the source line count"));
    }

    let mut generated_cursors = generated_lines
        .iter()
        .map(|(start, _)| *start)
        .collect::<Vec<_>>();

    let mut mappings = Vec::new();

    for (range_start, range_end) in crate::source::luau_ranges(source).map_err(io::Error::other)? {
        for (line, ((original_start, original_end), (generated_start, generated_end))) in
            original_lines.iter().zip(&generated_lines).enumerate()
        {
            let start = range_start.max(*original_start);
            let end = range_end.min(*original_end);

            if start >= end {
                continue;
            }

            let (start, end) = crate::source::trimmed(source, start, end);

            if start == end {
                continue;
            }

            let text = &source[start..end];
            let search_start = generated_cursors[line].max(*generated_start);

            let Some(relative) = generated[search_start..*generated_end].find(text) else {
                continue;
            };

            let generated_match = search_start + relative;
            let generated_match_end = generated_match + text.len();

            mappings.push(Mapping {
                start: generated_match,
                end: generated_match_end,
                original_start: start,
                original_end: end,
            });

            generated_cursors[line] = generated_match_end;
        }
    }

    Ok(mappings)
}

fn lines(source: &str) -> Vec<(usize, usize)> {
    let mut start = 0;

    source
        .split_inclusive('\n')
        .map(|line| {
            let range = (start, start + line.len());
            start = range.1;

            range
        })
        .collect()
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
