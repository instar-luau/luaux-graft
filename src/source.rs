use luaux::markup::{Node as LuauxNode, Span, parse_node};
use vermis::{Kind, Tree};

pub(crate) struct SourceFile {
    pub(crate) segments: Vec<Segment>,
    pub(crate) hole_starts: Vec<usize>,
}

pub(crate) struct Segment {
    pub(crate) node: LuauxNode,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

impl SourceFile {
    pub(crate) fn markup_nodes(&self) -> impl Iterator<Item = &LuauxNode> {
        self.segments.iter().map(|segment| &segment.node)
    }
}

pub(crate) enum RangeKind {
    Luau,
    Markup(LuauxNode),
}

pub(crate) fn hole_inner(source: &str, span: Span) -> (usize, usize) {
    trimmed(source, span.start + 1, span.end - 1)
}

pub(crate) fn brace_span(source: &str, span: Span) -> Span {
    let text = &source[span.start..span.end];

    Span::new(span.start + text.find('{').unwrap_or(0), span.end)
}

pub(crate) fn trimmed(source: &str, start: usize, end: usize) -> (usize, usize) {
    let text = &source[start..end];
    let front = text.len() - text.trim_start().len();
    let back = text.len() - text.trim_end().len();

    if front + back >= text.len() {
        (start, start)
    } else {
        (start + front, end - back)
    }
}

pub(crate) fn parse_range(source: &str, start: usize, end: usize) -> Option<RangeKind> {
    if start == end {
        return Some(RangeKind::Luau);
    }

    if let Ok((node, node_end)) = parse_node(source, start)
        && node_end == end
    {
        return Some(RangeKind::Markup(node));
    }

    vermis::parse((&source.as_bytes()[start..end]).into())
        .diagnostics
        .is_empty()
        .then_some(RangeKind::Luau)
}

pub(crate) fn parse(source: &str) -> Result<SourceFile, String> {
    let tree = vermis::parse_luaux(source.as_bytes().into());

    if let Some(diagnostic) = tree.diagnostics.first() {
        return Err(format!(
            "byte {}: {}",
            diagnostic.span.start, diagnostic.message
        ));
    }

    let mut markup = Vec::new();
    collect_markup(&tree, source, tree.root, &mut markup)?;

    markup.sort_by_key(|segment| segment.start);

    let hole_starts = tree
        .nodes
        .iter()
        .filter(|node| node.kind == Kind::MarkupExpression)
        .map(|node| trimmed_start(source, node.span))
        .collect();

    Ok(SourceFile {
        segments: markup,
        hole_starts,
    })
}

fn collect_markup(
    tree: &Tree<'_>,
    source: &str,
    index: usize,
    markup: &mut Vec<Segment>,
) -> Result<(), String> {
    let Some(node) = tree.nodes.get(index) else {
        return Ok(());
    };

    if matches!(node.kind, Kind::Element | Kind::Fragment) {
        let (markup_node, end) =
            parse_node(source, node.span.start).map_err(|error| error.message)?;

        markup.push(Segment {
            node: markup_node,
            start: node.span.start,
            end,
        });

        return Ok(());
    }

    for child in tree
        .children
        .get(node.children.clone())
        .into_iter()
        .flatten()
    {
        collect_markup(tree, source, *child, markup)?;
    }

    Ok(())
}

fn trimmed_start(source: &str, span: vermis::Span) -> usize {
    let start = span.start + 1;

    start + source[start..span.end.saturating_sub(1)].len()
        - source[start..span.end.saturating_sub(1)].trim_start().len()
}
