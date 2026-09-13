use crate::{
    protocol::Document,
    settings::{FormatOptions, QuoteStyle, TextWrap},
    source::{self, Segment},
};

use luaux::markup::{Attribute, AttributeValue, Child, Element, Node, Span};

struct Layout<'source> {
    source: &'source str,
    options: FormatOptions,
}

pub(crate) fn format(source: &str, options: FormatOptions) -> Result<Document, String> {
    let file = source::parse(source)?;
    let layout = Layout { source, options };

    let markup = file
        .segments
        .iter()
        .filter_map(|segment| match segment {
            Segment::Markup { start, end, .. } => Some((*start, *end)),
            Segment::Luau { .. } => None,
        })
        .collect::<Vec<_>>();

    let statements = file
        .statement_ranges()
        .into_iter()
        .filter(|(start, end)| !markup.iter().any(|(from, to)| *from < *end && *start < *to))
        .collect::<Vec<_>>();

    let mut parts = Vec::new();
    let mut cursor = 0;

    for (start, end) in statements {
        if start < cursor || end > source.len() {
            continue;
        }

        append_markup(&layout, cursor, start, &file.segments, &mut parts);
        parts.push(Document::host(start, end));
        cursor = end;
    }

    append_markup(&layout, cursor, source.len(), &file.segments, &mut parts);

    Ok(Document::concatenate(parts))
}

fn append_markup(
    layout: &Layout<'_>,
    start: usize,
    end: usize,
    segments: &[Segment],
    parts: &mut Vec<Document>,
) {
    let mut cursor = start;

    for segment in segments {
        let Segment::Markup {
            node,
            start: markup_start,
            end: markup_end,
        } = segment
        else {
            continue;
        };

        if *markup_start < start || *markup_end > end {
            continue;
        }

        if cursor < *markup_start {
            parts.push(Document::source(cursor, *markup_start));
        }

        parts.push(indented(layout, *markup_start, node_document(layout, node)));
        cursor = *markup_end;
    }

    if cursor < end {
        parts.push(Document::source(cursor, end));
    }
}

fn indented(layout: &Layout<'_>, start: usize, mut document: Document) -> Document {
    let line_start = layout.source[..start]
        .rfind('\n')
        .map_or(0, |index| index + 1);

    let indentation = &layout.source[line_start..start];

    let levels = indentation.matches('\t').count()
        + indentation.matches(' ').count() / layout.options.indent_width.max(1);

    for _ in 0..levels {
        document = Document::indent(document);
    }

    document
}

fn node_document(layout: &Layout<'_>, node: &Node) -> Document {
    match node {
        Node::Element(element) => element_document(layout, element),
        Node::Fragment(fragment) => children_document(layout, "<", &[], &fragment.children, "</>"),
    }
}

fn element_document(layout: &Layout<'_>, element: &Element) -> Document {
    let name = element.name.as_written();
    let source = &layout.source[element.span.start..element.span.end];

    if element.children.is_empty() && source.ends_with("/>") {
        return self_closing_document(layout, format!("<{name}"), &element.attributes);
    }

    children_document(
        layout,
        format!("<{name}"),
        &element.attributes,
        &element.children,
        format!("</{name}>"),
    )
}

fn self_closing_document(
    layout: &Layout<'_>,
    opening: String,
    attributes: &[Attribute],
) -> Document {
    let separator = match (layout.options.self_closing_space, attributes.is_empty()) {
        (true, true) => Document::literal(" "),
        (true, false) => Document::Line,
        (false, true) => Document::Nil,
        (false, false) => Document::Soft,
    };

    Document::group(Document::concatenate([
        Document::literal(opening),
        attributes_document(layout, attributes),
        separator,
        Document::literal("/>"),
    ]))
}

fn children_document(
    layout: &Layout<'_>,
    opening: impl Into<String>,
    attributes: &[Attribute],
    children: &[Child],
    closing: impl Into<String>,
) -> Document {
    let mut content = Vec::new();
    let mut glue = false;
    let mut broken = false;
    let mut previous_end = None;

    for child in children {
        let span = child.span();

        let separator = if glue {
            Document::Nil
        } else if broken {
            Document::Hard
        } else if layout.options.blank_lines
            && previous_end.is_some_and(|previous| {
                layout.source[previous..span.start].matches('\n').count() >= 2
            })
        {
            Document::Blank
        } else {
            Document::Soft
        };

        match child {
            Child::Text { .. } => {
                let text = text_document(layout, span);
                content.push(separator);
                content.push(text.0);
                glue = text.1;
                broken = false;
            }

            Child::Node(node) => {
                content.push(separator);
                content.push(node_document(layout, node));
                glue = false;
                broken = false;
            }

            Child::Expression { span, .. } => {
                content.push(separator);
                content.push(hole_document(layout, *span));
                glue = false;
                broken = false;
            }

            Child::Comment { .. } => {
                let multiline = layout.source[span.start..span.end].contains('\n');

                content.push(if multiline { Document::Hard } else { separator });

                content.push(Document::source(span.start, span.end));
                glue = false;
                broken = multiline;
            }
        }

        previous_end = Some(span.end);
    }

    let before_closing = if glue {
        Document::Nil
    } else if broken {
        Document::Hard
    } else {
        Document::Soft
    };

    Document::group(Document::concatenate([
        Document::group(Document::concatenate([
            Document::literal(opening),
            attributes_document(layout, attributes),
            if layout.options.bracket_same_line {
                Document::Nil
            } else {
                Document::Soft
            },
            Document::literal(">"),
        ])),
        Document::indent(Document::concatenate(content)),
        before_closing,
        Document::literal(closing),
    ]))
}

fn attributes_document(layout: &Layout<'_>, attributes: &[Attribute]) -> Document {
    if attributes.is_empty() {
        return Document::Nil;
    }

    let separator = if layout.options.attribute_per_line && attributes.len() > 1 {
        Document::Hard
    } else {
        Document::Line
    };

    let mut parts = Vec::new();

    for attribute in attributes {
        parts.push(separator.clone());
        parts.push(attribute_document(layout, attribute));
    }

    Document::indent(Document::concatenate(parts))
}

fn attribute_document(layout: &Layout<'_>, attribute: &Attribute) -> Document {
    match attribute {
        Attribute::Named { name, value, span } => match value {
            AttributeValue::Boolean => Document::literal(name.clone()),

            AttributeValue::StringLiteral(literal) => Document::concatenate([
                Document::literal(format!("{name}=")),
                string_document(layout, span.end - literal.len(), span.end),
            ]),

            AttributeValue::Expression(_) => Document::concatenate([
                Document::literal(format!("{name}=")),
                hole_document(layout, crate::source::brace_span(layout.source, *span)),
            ]),
        },

        Attribute::Spread { span, .. } => hole_document(layout, *span),

        Attribute::Inferred { span, .. } => Document::concatenate([
            Document::literal("="),
            hole_document(layout, crate::source::brace_span(layout.source, *span)),
        ]),
    }
}

fn string_document(layout: &Layout<'_>, start: usize, end: usize) -> Document {
    let Some(quote) = (match layout.options.attribute_quotes {
        QuoteStyle::Double => Some(b'"'),
        QuoteStyle::Single => Some(b'\''),
        QuoteStyle::Preserve => None,
    }) else {
        return Document::source(start, end);
    };

    let bytes = layout.source.as_bytes();

    if bytes[start] == quote || contains_unescaped(&bytes[start + 1..end - 1], quote) {
        return Document::source(start, end);
    }

    Document::concatenate([
        Document::literal((quote as char).to_string()),
        Document::source(start + 1, end - 1),
        Document::literal((quote as char).to_string()),
    ])
}

fn contains_unescaped(bytes: &[u8], quote: u8) -> bool {
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index += 2;
        } else if bytes[index] == quote {
            return true;
        } else {
            index += 1;
        }
    }

    false
}

fn hole_document(layout: &Layout<'_>, span: Span) -> Document {
    let (start, end) = crate::source::hole_inner(layout.source, span);

    let space = if layout.options.space_inside_braces {
        Document::literal(" ")
    } else {
        Document::Nil
    };

    let body = match crate::source::parse_range(layout.source, start, end) {
        Some(crate::source::RangeKind::Markup(node)) => node_document(layout, &node),
        Some(crate::source::RangeKind::Luau) => Document::expression(start, end),
        None => Document::source(start, end),
    };

    Document::concatenate([
        Document::literal("{"),
        space.clone(),
        body,
        space,
        Document::literal("}"),
    ])
}

struct TextDocument(Document, bool);

fn text_document(layout: &Layout<'_>, span: Span) -> TextDocument {
    let raw = &layout.source[span.start..span.end];
    let lines = raw.split('\n').collect::<Vec<_>>();
    let mut parts = Vec::new();
    let mut sticky_left = false;
    let mut sticky_right = false;
    let last = lines.len() - 1;
    let mut line_start = span.start;

    for (index, line) in lines.iter().enumerate() {
        let mut start = line_start;
        let mut end = line_start + line.len();

        if index > 0 {
            start += line.len() - line.trim_start().len();
        }

        if index < last {
            end -= line.len() - line.trim_end().len();
        }

        if start < end {
            if index == 0 && line.starts_with(char::is_whitespace) {
                sticky_left = true;
            }

            if index == last && line.ends_with(char::is_whitespace) {
                sticky_right = true;
            }

            if matches!(layout.options.text_wrap, TextWrap::Preserve) {
                parts.push((start, end));
            } else {
                let bytes = layout.source.as_bytes();
                let mut word_start = start;

                for index in start + 1..end.saturating_sub(1) {
                    if bytes[index] == b' '
                        && !bytes[index - 1].is_ascii_whitespace()
                        && !bytes[index + 1].is_ascii_whitespace()
                    {
                        parts.push((word_start, index));
                        word_start = index + 1;
                    }
                }

                parts.push((word_start, end));
            }
        }

        line_start += line.len() + 1;
    }

    let mut document = Document::Nil;

    for (index, (start, end)) in parts.into_iter().enumerate() {
        let part = Document::source(start, end);

        document = if index == 0 {
            part
        } else if matches!(layout.options.text_wrap, TextWrap::Preserve) {
            Document::concatenate([document, Document::Hard, part])
        } else {
            Document::concatenate([
                document,
                Document::group(Document::concatenate([Document::Line, part])),
            ])
        };
    }

    TextDocument(document, sticky_right || sticky_left)
}
