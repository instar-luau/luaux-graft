//! Native `LuauX` compilation, formatting, and linting graft for Instar.

mod compiler;
mod formatter;
mod linter;
mod protocol;
mod settings;
mod source;

use protocol::Request;

use std::{
    error::Error,
    io::{self, Read, Write},
    process::ExitCode,
};

const PAYLOAD_LIMIT: u64 = 64 * 1024 * 1024;

fn run() -> Result<(), Box<dyn Error>> {
    let mut input = Vec::new();

    io::stdin()
        .lock()
        .take(PAYLOAD_LIMIT + 1)
        .read_to_end(&mut input)?;

    if input.len() as u64 > PAYLOAD_LIMIT {
        return Err("graft request exceeds the payload limit".into());
    }

    let request: Request = serde_json::from_slice(&input)?;

    let output = match request.hook.as_str() {
        "compile" => serde_json::to_vec(&compiler::compile(&request)?)?,

        "format" => {
            if request.version != 1 || request.hook != "format" {
                return Err("expected a protocol 1 format request".into());
            }

            let options =
                settings::format_options(&request.configuration, request.settings.as_ref())?;

            serde_json::to_vec(&protocol::Format {
                version: 1,
                document: formatter::format(&request.source, options)?,
            })?
        }

        "lint" => {
            if request.version != 1 || request.hook != "lint" {
                return Err("expected a protocol 1 lint request".into());
            }

            let configuration = compiler::configuration(&request.configuration)?;

            serde_json::to_vec(&linter::lint(&request.source, &configuration)?)?
        }

        _ => return Err("unsupported graft hook".into()),
    };

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
    use super::protocol::{Mapping, offset};

    #[test]
    fn mappings_anchor_transformed_lines() {
        let source = "-- 雪\r\nreturn <Frame />\n";
        let generated = "-- 雪\r\nreturn React.createElement(\"Frame\")\n";
        let mut source_lines = source.split_inclusive('\n');
        let mut generated_lines = generated.split_inclusive('\n');
        let mut source_start = 0;
        let mut generated_start = 0;
        let mut mappings = Vec::<Mapping>::new();

        while let (Some(original), Some(output)) = (source_lines.next(), generated_lines.next()) {
            let generated_end = generated_start + output.len();

            mappings.push(Mapping {
                start: generated_start,
                end: generated_end,
                original_start: source_start,
                original_end: if original == output {
                    source_start + original.len()
                } else {
                    source_start
                },
            });

            source_start += original.len();
            generated_start = generated_end;
        }

        assert_eq!(mappings[0].original_end, offset("-- 雪\r\n".len()) as usize);
        assert_eq!(mappings[1].original_start, mappings[1].original_end);
    }
}
