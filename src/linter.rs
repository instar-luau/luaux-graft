use crate::{compiler, protocol::Finding, source::SourceFile};

use luaux::{
    Config,
    compile::{CompileError, Warning, compile_recovering},
    config::LintLevel,
    markup::{Attribute, AttributeValue, Child, Node},
};

pub(crate) fn lint(source: &str, configuration: &Config) -> Vec<Finding> {
    let Ok(file) = crate::source::parse(source) else {
        return Vec::new();
    };

    let mut findings = Vec::new();

    for node in file.markup_nodes() {
        check_node(source, node, &mut findings);
    }

    let mut compiler_configuration = configuration.clone();
    compiler_configuration.static_conditional_child = LintLevel::Warn;

    match compile_recovering(
        source,
        compiler::backend(&compiler_configuration).as_ref(),
        compiler_configuration,
    ) {
        Ok(compiled) => {
            for warning in &compiled.warnings {
                findings.push(compiler_finding(source, &file, warning));
            }

            for error in &compiled.errors {
                findings.push(error_finding(source, &file, error));
            }
        }

        Err(error) => findings.push(Finding {
            rule: "compile_error".into(),
            message: error.message.clone(),
            start: mark_start(source, error.offset),
            end: mark_end(source, error.offset, error.length.max(1)),
        }),
    }

    findings.sort_by_key(|finding| finding.start);

    findings
}

fn check_node(source: &str, node: &Node, findings: &mut Vec<Finding>) {
    let (attributes, children) = match node {
        Node::Element(element) => (&element.attributes[..], &element.children[..]),
        Node::Fragment(fragment) => (&[][..], &fragment.children[..]),
    };

    let mut names = Vec::new();

    for attribute in attributes {
        match attribute {
            Attribute::Named { name, span, value } => {
                if names.iter().any(|seen| *seen == name) {
                    findings.push(finding(
                        "duplicate_attribute",
                        *span,
                        format!("duplicate attribute {name}"),
                    ));
                } else {
                    names.push(name.as_str());
                }

                if matches!(value, AttributeValue::Expression(expression) if expression == "true") {
                    findings.push(finding(
                        "explicit_true_attribute",
                        *span,
                        format!("{name}={{true}} can be written as {name}"),
                    ));
                }
            }

            Attribute::Spread { .. } | Attribute::Inferred { .. } => {}
        }
    }

    for child in children {
        match child {
            Child::Node(child) => {
                if matches!(child, Node::Fragment(_)) {
                    findings.push(finding(
                        "useless_fragment",
                        node_span(child),
                        "fragment adds no level",
                    ));
                }

                check_node(source, child, findings);
            }

            Child::Text { text, span } if text.starts_with("--") || text.starts_with("//") => {
                findings.push(finding(
                    "comment_as_text",
                    *span,
                    "comment-like text is displayed",
                ));
            }

            Child::Expression { .. } | Child::Text { .. } | Child::Comment { .. } => {}
        }
    }

    if let Node::Element(element) = node
        && element.children.is_empty()
        && !source[element.span.start..element.span.end].ends_with("/>")
    {
        findings.push(finding(
            "self_closing_element",
            element.span,
            "empty element can self-close",
        ));
    }
}

fn node_span(node: &Node) -> luaux::markup::Span {
    match node {
        Node::Element(element) => element.span,
        Node::Fragment(fragment) => fragment.span,
    }
}

fn compiler_finding(source: &str, file: &SourceFile, warning: &Warning) -> Finding {
    let rule = if warning.message.contains("built once") {
        "static_conditional_child"
    } else {
        "compile_warning"
    };

    let (start, end) = locate(
        source,
        file,
        warning.offset,
        warning.length,
        &warning.message,
    );

    Finding {
        rule: rule.into(),
        message: warning.message.clone(),
        start,
        end,
    }
}

fn error_finding(source: &str, file: &SourceFile, error: &CompileError) -> Finding {
    let (start, end) = locate(source, file, error.offset, error.length, &error.message);

    Finding {
        rule: "unresolved_name".into(),
        message: error.message.clone(),
        start,
        end,
    }
}

fn locate(
    source: &str,
    file: &SourceFile,
    offset: usize,
    length: usize,
    message: &str,
) -> (usize, usize) {
    if fits(source, offset, offset + length, message) {
        return (mark_start(source, offset), mark_end(source, offset, length));
    }

    for hole in &file.hole_starts {
        let start = hole + offset;

        if fits(source, start, start + length, message) {
            return (mark_start(source, start), mark_end(source, start, length));
        }
    }

    (mark_start(source, offset), mark_end(source, offset, length))
}

fn fits(source: &str, start: usize, end: usize, message: &str) -> bool {
    end <= source.len()
        && source
            .get(start..end)
            .is_some_and(|text| message.contains(text))
}

fn finding(rule: &str, span: luaux::markup::Span, message: impl Into<String>) -> Finding {
    Finding {
        rule: rule.into(),
        message: message.into(),
        start: span.start,
        end: span.end,
    }
}

fn mark_start(source: &str, offset: usize) -> usize {
    let mut offset = offset.min(source.len());

    while !source.is_char_boundary(offset) {
        offset -= 1;
    }

    offset
}

fn mark_end(source: &str, offset: usize, length: usize) -> usize {
    let start = mark_start(source, offset);

    mark_start(
        source,
        start
            .saturating_add(length)
            .max(start + usize::from(start < source.len())),
    )
}
