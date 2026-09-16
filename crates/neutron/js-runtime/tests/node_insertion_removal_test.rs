use js_runtime::{Context, Runtime};

#[test]
fn prepend_and_append_insert_mixed_node_and_string_args_in_order() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("ul");
    d.set_attribute(parent, "id", "parent");
    let middle = d.create_element("li");
    d.set_attribute(middle, "id", "middle");
    d.append_child(root, parent);
    d.append_child(parent, middle);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const parent = document.getElementById('parent'); \
                const first = document.createElement('li'); \
                first.id = 'first'; \
                parent.prepend(first, 'lead-text'); \
                const last = document.createElement('li'); \
                last.id = 'last'; \
                parent.append('tail-text', last); \
                return Array.from(parent.childNodes).map(n => n.id || n.textContent).join(','); \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "first,lead-text,middle,tail-text,last");
}

#[test]
fn before_and_after_insert_relative_to_the_reference_node() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    d.set_attribute(parent, "id", "parent");
    let anchor = d.create_element("span");
    d.set_attribute(anchor, "id", "anchor");
    d.append_child(root, parent);
    d.append_child(parent, anchor);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const parent = document.getElementById('parent'); \
                const anchor = document.getElementById('anchor'); \
                const before1 = document.createElement('b'); \
                before1.id = 'before1'; \
                const before2 = document.createElement('b'); \
                before2.id = 'before2'; \
                anchor.before(before1, before2); \
                const after1 = document.createElement('i'); \
                after1.id = 'after1'; \
                const after2 = document.createElement('i'); \
                after2.id = 'after2'; \
                anchor.after(after1, after2); \
                const detached = document.createElement('u'); \
                let detachedNoOp = true; \
                detached.before(document.createElement('span')); \
                detachedNoOp = detached.parentNode === null; \
                return `${Array.from(parent.childNodes).map(n => n.id).join(',')},${detachedNoOp}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "before1,before2,anchor,after1,after2,true");
}

#[test]
fn replace_with_swaps_position_and_keeps_the_old_node_alive() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    d.set_attribute(parent, "id", "parent");
    let first = d.create_element("span");
    d.set_attribute(first, "id", "first");
    let old = d.create_element("span");
    d.set_attribute(old, "id", "old");
    let last = d.create_element("span");
    d.set_attribute(last, "id", "last");
    d.append_child(root, parent);
    d.append_child(parent, first);
    d.append_child(parent, old);
    d.append_child(parent, last);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const old = document.getElementById('old'); \
                const a = document.createElement('em'); \
                a.id = 'a'; \
                const b = document.createElement('em'); \
                b.id = 'b'; \
                old.replaceWith(a, 'text-between', b); \
                const parent = document.getElementById('parent'); \
                const before = `${Array.from(parent.childNodes).map(n => n.id || n.textContent).join(',')},${document.getElementById('old')}`; \
                const other = document.createElement('section'); \
                parent.appendChild(other); \
                other.appendChild(old); \
                return `${before},${other.firstChild === old},${old.id}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "first,a,text-between,b,last,null,true,old");
}

/// Roadmap P0 item 1 (spec/architecture/primitives.md §4.1): a node
/// removed via `removeChild`/`remove` must not be destroyed — it keeps its
/// JS identity, its own expando properties, its event listeners, and its
/// own subtree, and can be reattached anywhere afterward. Covers both
/// removal entry points and both "detached" evidence (identity survives)
/// and "still alive" evidence (listener still fires, subtree intact).
#[test]
fn removed_nodes_are_detached_not_destroyed() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    d.set_attribute(parent, "id", "parent");
    let child = d.create_element("span");
    d.set_attribute(child, "id", "child");
    let grandchild = d.create_element("em");
    d.set_attribute(grandchild, "id", "grandchild");
    d.append_child(root, parent);
    d.append_child(parent, child);
    d.append_child(child, grandchild);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const parent = document.getElementById('parent'); \
                const child = document.getElementById('child'); \
                let fired = 0; \
                child.addEventListener('custom', () => { fired++; }); \
                child.marker = 'kept'; \
                const removed = parent.removeChild(child); \
                const sameIdentity = removed === child; \
                const stillHasSubtree = child.firstChild && child.firstChild.id === 'grandchild'; \
                const stillHasExpando = child.marker === 'kept'; \
                child.dispatchEvent(new Event('custom')); \
                const listenerSurvived = fired === 1; \
                const isDetached = child.parentNode === null && !parent.contains(child); \
                const other = document.createElement('section'); \
                parent.appendChild(other); \
                other.appendChild(child); \
                const reattached = other.firstChild === child && child.id === 'child'; \
                return [sameIdentity, stillHasSubtree, stillHasExpando, listenerSurvived, isDetached, reattached].join(','); \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,true,true,true,true,true");
}

#[test]
fn child_node_insertion_methods_reject_a_string_arg_treated_as_a_node_cycle() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    d.set_attribute(parent, "id", "parent");
    let child = d.create_element("span");
    d.set_attribute(child, "id", "child");
    d.append_child(root, parent);
    d.append_child(parent, child);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const parent = document.getElementById('parent'); \
                const child = document.getElementById('child'); \
                let cycleRejected = false; \
                try { child.append(parent); } catch (_) { cycleRejected = true; } \
                let badArgRejected = false; \
                try { parent.append(42); } catch (_) { badArgRejected = true; } \
                return `${cycleRejected},${badArgRejected},${parent.querySelector('#child') === child}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,true,true");
}

#[test]
fn insert_adjacent_html_inserts_at_all_four_real_positions() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let outer = d.create_element("section");
    d.set_attribute(outer, "id", "outer");
    let target = d.create_element("div");
    d.set_attribute(target, "id", "target");
    let existing_child = d.create_element("span");
    d.set_attribute(existing_child, "id", "existing");
    d.append_child(root, outer);
    d.append_child(outer, target);
    d.append_child(target, existing_child);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const target = document.getElementById('target'); \
                target.insertAdjacentHTML('beforebegin', '<p id=\"before\">before</p>'); \
                target.insertAdjacentHTML('afterend', '<p id=\"after\">after</p>'); \
                target.insertAdjacentHTML('afterbegin', '<b id=\"first\">first</b>'); \
                target.insertAdjacentHTML('beforeend', '<b id=\"last\">last</b>'); \
                const outer = document.getElementById('outer'); \
                return `${Array.from(outer.childNodes).map(n => n.id).join(',')},${Array.from(target.childNodes).map(n => n.id).join(',')}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "before,target,after,first,existing,last");
}

#[test]
fn insert_adjacent_html_rejects_bad_position_and_no_parent_cases() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let target = d.create_element("div");
    d.set_attribute(target, "id", "target");
    d.append_child(root, target);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const target = document.getElementById('target'); \
                let badPositionRejected = false; \
                try { target.insertAdjacentHTML('nowhere', '<b></b>'); } catch (_) { badPositionRejected = true; } \
                const detached = document.createElement('div'); \
                let noParentRejected = false; \
                try { detached.insertAdjacentHTML('beforebegin', '<b></b>'); } catch (_) { noParentRejected = true; } \
                detached.insertAdjacentHTML('afterbegin', '<b id=\"ok\">ok</b>'); \
                return `${badPositionRejected},${noParentRejected},${detached.firstChild.id}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,true,ok");
}

#[test]
fn compare_document_position_returns_following_for_later_sibling() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let a = d.create_element("div");
    let b = d.create_element("span");
    d.set_attribute(a, "id", "a");
    d.set_attribute(b, "id", "b");
    d.append_child(root, a);
    d.append_child(root, b);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const a = document.getElementById('a'); const b = document.getElementById('b'); return `${a.compareDocumentPosition(b) & 4},${b.compareDocumentPosition(a) & 2},${a.compareDocumentPosition(a)}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "4,2,0");
}

#[test]
fn compare_document_position_contains_and_contained_by() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let child = d.create_element("span");
    d.set_attribute(parent, "id", "parent");
    d.set_attribute(child, "id", "child");
    d.append_child(root, parent);
    d.append_child(parent, child);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const p = document.getElementById('parent'); const c = document.getElementById('child'); return `${p.compareDocumentPosition(c) & 16},${c.compareDocumentPosition(p) & 8}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "16,8");
}

#[test]
fn compare_document_position_disconnected_nodes() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let a = d.create_element("div");
    let b = d.create_element("span");
    d.set_attribute(a, "id", "a");
    d.set_attribute(b, "id", "b");
    d.append_child(root, a);
    // b is detached

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const a = document.getElementById('a'); const b = document.getElementById('b'); return `${a.compareDocumentPosition(b) & 1}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "1");
}
