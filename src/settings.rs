use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "format controls are independent"
)]
pub(crate) struct FormatOptions {
    pub(crate) attribute_quotes: QuoteStyle,
    pub(crate) bracket_same_line: bool,
    pub(crate) attribute_per_line: bool,
    pub(crate) self_closing_space: bool,
    pub(crate) text_wrap: TextWrap,
    pub(crate) blank_lines: bool,
    pub(crate) space_inside_braces: bool,
}

#[derive(Clone, Copy)]
pub(crate) enum QuoteStyle {
    Double,
    Single,
    Preserve,
}

#[derive(Clone, Copy)]
pub(crate) enum TextWrap {
    Fill,
    Preserve,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            attribute_quotes: QuoteStyle::Double,
            bracket_same_line: false,
            attribute_per_line: false,
            self_closing_space: true,
            text_wrap: TextWrap::Fill,
            blank_lines: true,
            space_inside_braces: true,
        }
    }
}

pub(crate) fn format_options(
    configuration: &BTreeMap<String, Value>,
    settings: Option<&Value>,
) -> Result<FormatOptions, String> {
    let mut options = FormatOptions::default();
    let mut values = Map::new();

    if let Some(settings) = settings.and_then(Value::as_object)
        && let Some(braces) = settings
            .get("spacing")
            .and_then(|spacing| spacing.get("braces"))
    {
        values.insert("space_inside_braces".into(), braces.clone());
    }

    let own = configuration
        .get("format")
        .and_then(Value::as_object)
        .or_else(|| {
            configuration
                .get("format_options")
                .and_then(Value::as_object)
        });

    if let Some(own) = own {
        values.extend(own.clone());
    }

    for (name, value) in values {
        match name.as_str() {
            "attribute_quotes" => {
                options.attribute_quotes = match value.as_str() {
                    Some("double") => QuoteStyle::Double,
                    Some("single") => QuoteStyle::Single,
                    Some("preserve") => QuoteStyle::Preserve,
                    _ => return Err("attribute_quotes must be double, single, or preserve".into()),
                };
            }

            "text_wrap" => {
                options.text_wrap = match value.as_str() {
                    Some("fill") => TextWrap::Fill,
                    Some("preserve") => TextWrap::Preserve,
                    _ => return Err("text_wrap must be fill or preserve".into()),
                };
            }

            "bracket_same_line" => options.bracket_same_line = boolean(&name, &value)?,
            "attribute_per_line" => options.attribute_per_line = boolean(&name, &value)?,
            "self_closing_space" => options.self_closing_space = boolean(&name, &value)?,
            "blank_lines" => options.blank_lines = boolean(&name, &value)?,
            "space_inside_braces" => options.space_inside_braces = boolean(&name, &value)?,
            _ => {}
        }
    }

    Ok(options)
}

fn boolean(name: &str, value: &Value) -> Result<bool, String> {
    value
        .as_bool()
        .ok_or_else(|| format!("{name} must be true or false"))
}
