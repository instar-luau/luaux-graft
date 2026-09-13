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

    if source::luau_ranges(&request.source).is_err() {
        let output = match request.hook.as_str() {
            "compile" => serde_json::to_vec(&protocol::Compilation {
                version: 1,
                source: String::new(),
                dependencies: Vec::new(),
                mappings: Vec::new(),
            })?,

            "format" => serde_json::to_vec(&protocol::Format {
                version: 1,
                document: protocol::Document::source(0, request.source.len()),
            })?,

            "lint" => serde_json::to_vec(&Vec::<protocol::Finding>::new())?,
            _ => return Err("unsupported graft hook".into()),
        };

        io::stdout().lock().write_all(&output)?;

        return Ok(());
    }

    let output = match request.hook.as_str() {
        "compile" => serde_json::to_vec(&compiler::compile(&request)?)?,

        "format" => {
            if request.version != 1 || request.hook != "format" {
                return Err("expected a protocol 1 format request".into());
            }

            let options = settings::format_options(&request.configuration)?;

            serde_json::to_vec(&protocol::Format {
                version: 1,
                document: formatter::format(&request.source, options),
            })?
        }

        "lint" => {
            if request.version != 1 || request.hook != "lint" {
                return Err("expected a protocol 1 lint request".into());
            }

            let configuration = compiler::configuration(&request)?;

            serde_json::to_vec(&linter::lint(&request.source, &configuration))?
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
    #[test]
    fn mappings_cover_only_preserved_luau() {
        let mappings = super::compiler::mappings(
            "-- 雪\r\nreturn <Frame />\n",
            "-- 雪\r\nreturn React.createElement(\"Frame\")\n",
        )
        .unwrap();

        assert!(!mappings.is_empty());

        for mapping in mappings {
            assert_eq!(
                &"-- 雪\r\nreturn React.createElement(\"Frame\")\n"[mapping.start..mapping.end],
                &"-- 雪\r\nreturn <Frame />\n"[mapping.original_start..mapping.original_end]
            );
        }
    }
}
