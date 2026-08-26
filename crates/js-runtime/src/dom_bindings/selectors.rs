//! CSS selector matching against the live DOM: builds an `css::ElementSnapshot`
//! ancestry chain for a node (`selector_chain` and its helpers) and walks the
//! tree collecting matches (`matching_nodes`/`matching_by_tag`/
//! `matching_by_class`/`descendants_matching_tags`), plus the single-node
//! `node_matches_selector` check `matches()`/`closest()` build on.

const MAX_SELECTOR_LENGTH: usize = 1024;
/// Also used by `collections.rs`'s `node_closest` to bound its ancestor walk.
pub(super) const MAX_SELECTOR_VISITS: usize = 4096;
const MAX_SELECTOR_RESULTS: usize = 2048;

fn element_snapshot(dom: &dom::Dom, id: dom::NodeId) -> Option<css::ElementSnapshot> {
    let node = dom.get(id)?;
    let dom::NodeData::Element {
        tag, attributes, ..
    } = &node.data
    else {
        return None;
    };
    Some(css::ElementSnapshot {
        tag: tag.clone(),
        id: attributes.get("id").cloned(),
        classes: attributes
            .get("class")
            .map(|value| value.split_ascii_whitespace().map(str::to_owned).collect())
            .unwrap_or_default(),
        attributes: attributes
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        preceding_siblings: Vec::new(),
        has_following_sibling: false,
    })
}

/// Builds the finite left-sibling chain needed by `+`/`~` matching. A
/// sibling snapshot never recursively includes following siblings, avoiding
/// a `first -> second -> first` cycle while retaining `a + b + c` support.
fn preceding_snapshot(dom: &dom::Dom, id: dom::NodeId) -> Option<css::ElementSnapshot> {
    let mut result = element_snapshot(dom, id)?;
    let parent = dom.get(id)?.parent?;
    let siblings = &dom.get(parent)?.children;
    let position = siblings.iter().position(|&sibling| sibling == id)?;
    result.preceding_siblings = siblings[..position]
        .iter()
        .filter_map(|&sibling| preceding_snapshot(dom, sibling))
        .collect();
    Some(result)
}

fn snapshot(dom: &dom::Dom, id: dom::NodeId) -> Option<css::ElementSnapshot> {
    let mut result = preceding_snapshot(dom, id)?;
    let parent = dom.get(id)?.parent?;
    let siblings = &dom.get(parent)?.children;
    let position = siblings.iter().position(|&sibling| sibling == id)?;
    result.has_following_sibling = siblings[position + 1..]
        .iter()
        .any(|&sibling| element_snapshot(dom, sibling).is_some());
    Some(result)
}

fn selector_chain(dom: &dom::Dom, id: dom::NodeId) -> Option<Vec<css::ElementSnapshot>> {
    let mut ids = Vec::new();
    let mut current = Some(id);
    while let Some(node_id) = current {
        if matches!(dom.get(node_id)?.data, dom::NodeData::Element { .. }) {
            ids.push(node_id);
        }
        current = dom.get(node_id)?.parent;
    }
    ids.reverse();
    ids.into_iter()
        .map(|node_id| snapshot(dom, node_id))
        .collect()
}

pub(super) fn matching_nodes(
    dom: &dom::Dom,
    start: dom::NodeId,
    selector: &str,
    include_start: bool,
) -> Result<Vec<dom::NodeId>, &'static str> {
    if selector.is_empty() || selector.len() > MAX_SELECTOR_LENGTH {
        return Err("selector is empty or exceeds the maximum length");
    }
    let stylesheet = css::parse_stylesheet(&format!("{selector} {{}}"));
    let Some(rule) = stylesheet.rules.first() else {
        return Err("unsupported selector syntax");
    };
    if stylesheet.rules.len() != 1 || rule.selectors.0.is_empty() {
        return Err("unsupported selector syntax");
    }
    let mut result = Vec::new();
    let mut stack = vec![start];
    let mut visits = 0usize;
    while let Some(node_id) = stack.pop() {
        visits += 1;
        if visits > MAX_SELECTOR_VISITS {
            return Err("selector traversal limit exceeded");
        }
        let node = dom.get(node_id).ok_or("invalid DOM node")?;
        if (include_start || node_id != start) && matches!(node.data, dom::NodeData::Element { .. })
        {
            let chain = selector_chain(dom, node_id).ok_or("invalid DOM ancestry")?;
            if rule
                .selectors
                .0
                .iter()
                .any(|candidate| css::selector_matches(candidate, &chain))
            {
                result.push(node_id);
                if result.len() > MAX_SELECTOR_RESULTS {
                    return Err("selector result limit exceeded");
                }
            }
        }
        stack.extend(node.children.iter().rev().copied());
    }
    Ok(result)
}

/// `document`/`Node.prototype`'s `getElementsByTagName`, matched directly
/// against each `Element`'s stored tag rather than routed through the CSS
/// selector engine (`matching_nodes`) — a plain string compare, so a tag
/// argument containing selector metacharacters (`.`, `#`, `,`, ...) can
/// never be reinterpreted as different selector syntax. `"*"` matches every
/// element, mirroring the real API's universal case. Case-sensitive, same
/// as this engine's CSS type-selector matching (`cascade.rs`'s
/// `SimpleSelector::Type`) — a real browser's HTML-document case
/// insensitivity isn't modeled here, consistent with that existing gap.
pub(super) fn matching_by_tag(
    dom: &dom::Dom,
    start: dom::NodeId,
    tag: &str,
    include_start: bool,
) -> Result<Vec<dom::NodeId>, &'static str> {
    let mut result = Vec::new();
    let mut stack = vec![start];
    let mut visits = 0usize;
    while let Some(node_id) = stack.pop() {
        visits += 1;
        if visits > MAX_SELECTOR_VISITS {
            return Err("traversal limit exceeded");
        }
        let node = dom.get(node_id).ok_or("invalid DOM node")?;
        if include_start || node_id != start {
            if let dom::NodeData::Element { tag: node_tag, .. } = &node.data {
                if tag == "*" || node_tag == tag {
                    result.push(node_id);
                    if result.len() > MAX_SELECTOR_RESULTS {
                        return Err("result limit exceeded");
                    }
                }
            }
        }
        stack.extend(node.children.iter().rev().copied());
    }
    Ok(result)
}

/// `document`/`Node.prototype`'s `getElementsByClassName`, matched by real
/// token-set membership (every whitespace-separated token in `class_name`
/// must be present in an element's own `class` attribute tokens) — the
/// real spec algorithm, not a CSS class-selector compound built and run
/// through the selector engine, which would let a class name containing a
/// selector metacharacter (a literal `.`/`#`/`,` in an authored class
/// token, valid HTML even if unusual) be misinterpreted as more selector
/// syntax instead of one literal token.
pub(super) fn matching_by_class(
    dom: &dom::Dom,
    start: dom::NodeId,
    class_name: &str,
    include_start: bool,
) -> Result<Vec<dom::NodeId>, &'static str> {
    let wanted = super::class_list::class_tokens(class_name);
    if wanted.is_empty() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    let mut stack = vec![start];
    let mut visits = 0usize;
    while let Some(node_id) = stack.pop() {
        visits += 1;
        if visits > MAX_SELECTOR_VISITS {
            return Err("traversal limit exceeded");
        }
        let node = dom.get(node_id).ok_or("invalid DOM node")?;
        if include_start || node_id != start {
            if let dom::NodeData::Element { attributes, .. } = &node.data {
                let have = super::class_list::class_tokens(
                    attributes.get("class").map(String::as_str).unwrap_or(""),
                );
                if wanted.iter().all(|token| have.contains(token)) {
                    result.push(node_id);
                    if result.len() > MAX_SELECTOR_RESULTS {
                        return Err("result limit exceeded");
                    }
                }
            }
        }
        stack.extend(node.children.iter().rev().copied());
    }
    Ok(result)
}

pub(super) fn node_matches_selector(
    dom: &dom::Dom,
    id: dom::NodeId,
    selector: &str,
) -> Result<bool, &'static str> {
    if selector.is_empty() || selector.len() > MAX_SELECTOR_LENGTH {
        return Err("selector is empty or exceeds the maximum length");
    }
    if !matches!(
        dom.get(id).map(|node| &node.data),
        Some(dom::NodeData::Element { .. })
    ) {
        return Ok(false);
    }
    let stylesheet = css::parse_stylesheet(&format!("{selector} {{}}"));
    let Some(rule) = stylesheet.rules.first() else {
        return Err("unsupported selector syntax");
    };
    if stylesheet.rules.len() != 1 || rule.selectors.0.is_empty() {
        return Err("unsupported selector syntax");
    }
    let chain = selector_chain(dom, id).ok_or("invalid DOM ancestry")?;
    Ok(rule
        .selectors
        .0
        .iter()
        .any(|candidate| css::selector_matches(candidate, &chain)))
}

/// All element descendants of `start` whose tag is in `tags`, in document
/// order (iterative DFS, same traversal limits the selector engine uses).
pub(super) fn descendants_matching_tags(
    dom: &dom::Dom,
    start: dom::NodeId,
    tags: &[&str],
) -> Vec<dom::NodeId> {
    let mut result = Vec::new();
    let mut stack = vec![start];
    let mut visits = 0usize;
    while let Some(id) = stack.pop() {
        visits += 1;
        if visits > MAX_SELECTOR_VISITS || result.len() > MAX_SELECTOR_RESULTS {
            break;
        }
        if let Some(node) = dom.get(id) {
            if id != start {
                if let dom::NodeData::Element { tag, .. } = &node.data {
                    if tags.contains(&tag.as_str()) {
                        result.push(id);
                    }
                }
            }
            stack.extend(node.children.iter().rev().copied());
        }
    }
    result
}
