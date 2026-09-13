use luaux::markup::{Node as LuauxNode, Span, parse_node};
use vermis::{Kind, Tree};

pub(crate) struct SourceFile<'source> {
    tree: Tree<'source>,
    pub(crate) segments: Vec<Segment>,
    pub(crate) hole_starts: Vec<usize>,
}

pub(crate) enum Segment {
    Luau {
        start: usize,
    },

    Markup {
        node: LuauxNode,
        start: usize,
        end: usize,
    },
}

impl SourceFile<'_> {
    pub(crate) fn statement_ranges(&self) -> Vec<(usize, usize)> {
        let Some(root) = self.tree.nodes.get(self.tree.root) else {
            return Vec::new();
        };

        let Some(&block_index) = self
            .tree
            .children
            .get(root.children.clone())
            .and_then(|children| children.first())
        else {
            return Vec::new();
        };

        let Some(block) = self.tree.nodes.get(block_index) else {
            return Vec::new();
        };

        self.tree
            .children
            .get(block.children.clone())
            .into_iter()
            .flatten()
            .filter_map(|index| self.tree.nodes.get(*index))
            .filter(|node| node.kind != Kind::Error)
            .map(|node| (node.span.start, node.span.end))
            .collect()
    }

    pub(crate) fn markup_nodes(&self) -> impl Iterator<Item = &LuauxNode> {
        self.segments.iter().filter_map(|segment| match segment {
            Segment::Markup { node, .. } => Some(node),
            Segment::Luau { .. } => None,
        })
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

pub(crate) fn parse(source: &str) -> Result<SourceFile<'_>, String> {
    let tree = vermis::parse_luaux(source.as_bytes().into());

    if let Some(diagnostic) = tree.diagnostics.first() {
        return Err(format!(
            "byte {}: {}",
            diagnostic.span.start, diagnostic.message
        ));
    }

    let mut markup = Vec::new();
    collect_markup(&tree, source, tree.root, &mut markup)?;

    markup.sort_by_key(|segment: &Segment| match segment {
        Segment::Markup { start, .. } | Segment::Luau { start, .. } => *start,
    });

    let mut segments = Vec::new();
    let mut cursor = 0;

    for segment in markup {
        let (start, end) = match &segment {
            Segment::Markup { start, end, .. } => (*start, *end),
            Segment::Luau { .. } => continue,
        };

        if cursor < start {
            segments.push(Segment::Luau { start: cursor });
        }

        segments.push(segment);
        cursor = end;
    }

    if cursor < source.len() {
        segments.push(Segment::Luau { start: cursor });
    }

    let hole_starts = tree
        .nodes
        .iter()
        .filter(|node| node.kind == Kind::MarkupExpression)
        .map(|node| trimmed_start(source, node.span))
        .collect();

    Ok(SourceFile {
        tree,
        segments,
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

        markup.push(Segment::Markup {
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
