//! Protocol integration tests.

use serde_json::{Value, json};

use std::{
    io::Write,
    process::{Command, Output, Stdio},
};

fn invoke(request: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_graft"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    child.stdin.take().unwrap().write_all(request).unwrap();

    child.wait_with_output().unwrap()
}

fn request(source: &str, configuration: &Value) -> Value {
    hook_request("compile", source, configuration, &Value::Null)
}

fn hook_request(hook: &str, source: &str, configuration: &Value, settings: &Value) -> Value {
    json!({"version":1,"hook":hook,"source":source,"configuration":configuration,"settings":settings})
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
        let mut previous_original = 0;

        for mapping in result["mappings"].as_array().unwrap() {
            let start = usize::try_from(mapping["start"].as_u64().unwrap()).unwrap();
            let end = usize::try_from(mapping["end"].as_u64().unwrap()).unwrap();

            let original_start =
                usize::try_from(mapping["original_start"].as_u64().unwrap()).unwrap();

            let original_end = usize::try_from(mapping["original_end"].as_u64().unwrap()).unwrap();
            assert!(start >= previous);
            assert!(original_start >= previous_original);
            assert!(start < end);
            assert!(original_start < original_end);

            assert_eq!(
                generated.get(start..end),
                source.get(original_start..original_end)
            );

            previous = end;
            previous_original = original_end;
        }
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
    invalid_hook["hook"] = json!("unknown");
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

#[test]
fn formats_markup_with_the_layout_protocol() {
    let output = invoke(
        hook_request(
            "format",
            "return (<Frame Name='shop'/>)\n",
            &json!({"format":{"space_inside_braces":true}}),
            &json!({"indentation":{"width":4},"spacing":{"braces":false}}),
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
    assert_eq!(result["version"], 1);
    let document = result["document"].to_string();
    assert!(document.contains("Frame"));
    assert!(document.contains("template"));
    assert!(document.contains("sequence"));
    assert!(!document.contains("concat"));
    assert!(!document.contains("src"));
    assert!(!document.contains("lit"));
}

#[test]
fn lints_markup_with_source_ranges() {
    let output = invoke(
        hook_request(
            "lint",
            "return <Frame Name=\"a\" Name=\"b\"/>\n",
            &json!({}),
            &Value::Null,
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

    assert!(result.as_array().unwrap().iter().any(|finding| {
        finding["rule"] == "duplicate_attribute"
            && finding["start"].as_u64().unwrap() < finding["end"].as_u64().unwrap()
    }));
}
