//! Protocol integration tests.

use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Output, Stdio},
};

fn invoke(request: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_luaux-graft"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    child.stdin.take().unwrap().write_all(request).unwrap();

    child.wait_with_output().unwrap()
}

fn request(source: &str, configuration: &Value) -> Value {
    json!({"version":1,"hook":"compile","source":source,"configuration":configuration,"settings":null})
}

#[test]
fn compiles_react_with_valid_protocol_and_mappings() {
    for source in [
        "",
        "return 1",
        "-- 雪\r\nreturn 1\r\n",
        "local React = require('@react')\nreturn <Frame><TextLabel Text=\"雪\" /></Frame>\n",
        "local React = require('@react')\nlocal properties = {}\nreturn <Frame {...properties} />\n",
    ] {
        let output = invoke(request(source, &json!({})).to_string().as_bytes());

        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["version"], 1);
        assert_eq!(result["dependencies"], json!([]));
        let generated = result["source"].as_str().unwrap();

        if source.contains("<Frame") {
            assert!(generated.contains("React.createElement"), "{generated}");
        } else {
            assert_eq!(generated, source);
        }

        let mut previous = 0;

        for mapping in result["mappings"].as_array().unwrap() {
            let start = usize::try_from(mapping["start"].as_u64().unwrap()).unwrap();
            let end = usize::try_from(mapping["end"].as_u64().unwrap()).unwrap();

            let original_start =
                usize::try_from(mapping["original_start"].as_u64().unwrap()).unwrap();

            let original_end = usize::try_from(mapping["original_end"].as_u64().unwrap()).unwrap();
            assert_eq!(start, previous);
            assert!(start < end);
            assert!(generated.get(start..end).is_some());
            assert!(source.get(original_start..original_end).is_some());
            previous = end;
        }

        assert_eq!(previous, generated.len());
    }
}

#[test]
fn accepts_upstream_factory_configuration() {
    let source = "local create = require('@vide').create\nreturn <Frame />\n";

    let output = invoke(
        request(
            source,
            &json!({"factory":{"backend":"table","create":"create"}}),
        )
        .to_string()
        .as_bytes(),
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    let generated = result["source"].as_str().unwrap();
    assert!(generated.contains("create"), "{generated}");
    assert!(!generated.contains("React"), "{generated}");
}

#[test]
fn failures_leave_standard_output_empty() {
    let mut invalid_version = request("return 1", &json!({}));
    invalid_version["version"] = json!(2);
    let mut invalid_hook = request("return 1", &json!({}));
    invalid_hook["hook"] = json!("format");
    let mut invalid_settings = request("return 1", &json!({}));
    invalid_settings["settings"] = json!({});

    for input in [
        "not JSON".to_owned(),
        "{}".to_owned(),
        invalid_version.to_string(),
        invalid_hook.to_string(),
        invalid_settings.to_string(),
        request("return <Frame", &json!({})).to_string(),
        request("return 1", &json!({"unknown":true})).to_string(),
        request(
            "local React = require('@react')\nreturn <NonexistentClass />",
            &json!({}),
        )
        .to_string(),
    ] {
        let output = invoke(input.as_bytes());
        assert!(!output.status.success(), "{input}");
        assert_eq!(output.stdout, Vec::<u8>::new());
        assert_ne!(output.stderr, Vec::<u8>::new());
    }
}
