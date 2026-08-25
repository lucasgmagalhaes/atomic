use css::{parse_stylesheet, CompoundSelector, SimpleSelector, Token};

/// Real regression coverage for a genuine infinite-loop bug found via a
/// real fetched page (`https://example.com/`'s own actual CSS has
/// `a:link,a:visited{...}`): `parse_compound_selector` consumes tokens
/// (e.g. `a`, `:`, `link`) before discovering `link` isn't a supported
/// pseudo-class and returning `None` - the outer `parse_stylesheet`/
/// `parse_media_block` loops didn't advance past a failed rule at all,
/// so they retried `parse_rule` forever on the exact same stuck token.
/// Fixed by `skip_malformed_rule` (real CSS Syntax Module error recovery:
/// skip to the next `{`, then skip its balanced block). Runs each case on
/// a real background thread with a hard join timeout so a *future*
/// regression fails this test in a few seconds with a clear message,
/// instead of hanging the whole test binary (and, if hit through the real
/// pipeline, a live `profile-worker` process) indefinitely.
fn parse_with_timeout(css: &'static str) -> css::Stylesheet {
    let handle = std::thread::spawn(move || parse_stylesheet(css));
    let start = std::time::Instant::now();
    loop {
        if handle.is_finished() {
            return handle.join().expect("parse_stylesheet should not panic");
        }
        assert!(start.elapsed() < std::time::Duration::from_secs(5), "parse_stylesheet({css:?}) did not return within 5s - likely stuck in an infinite loop again");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn an_unrecognized_pseudo_class_does_not_hang_the_parser() {
    let sheet = parse_with_timeout("a:link{color:#348}");
    // Real CSS Syntax Module recovery: the whole malformed rule is
    // discarded (this parser doesn't support `:link` at all), not
    // partially kept - the important thing is that parsing completes and
    // moves on, not that this specific rule survives.
    assert_eq!(sheet.rules.len(), 0);
}

#[test]
fn an_unrecognized_pseudo_class_in_a_comma_list_does_not_hang_and_later_rules_still_parse() {
    let sheet = parse_with_timeout("a:link,a:visited{color:#348}div{width:10px}");
    assert_eq!(
        sheet.rules.len(),
        1,
        "the malformed rule should be skipped, but a real later rule must still parse"
    );
    assert_eq!(
        sheet.rules[0].selectors.0[0].0,
        vec![CompoundSelector(vec![SimpleSelector::Type("div".into())])]
    );
}

#[test]
fn the_real_example_com_stylesheet_parses_without_hanging() {
    // The exact real CSS `https://example.com/` serves - this is what
    // originally hung a real navigate() call end to end.
    let sheet = parse_with_timeout("body{background:#eee;width:60vw;margin:15vh auto;font-family:system-ui,sans-serif}h1{font-size:1.5em}div{opacity:0.8}a:link,a:visited{color:#348}");
    // 3 real rules parse (body/h1/div) - vw/em units and opacity aren't
    // supported either, but unsupported *values* don't fail a rule the
    // way an unsupported *selector* does, so these still count as parsed
    // rules even though their declarations may not all apply later.
    assert_eq!(sheet.rules.len(), 3);
}

#[test]
fn a_malformed_rule_inside_a_real_media_block_does_not_hang() {
    let sheet = parse_with_timeout("@media screen { a:link{color:red} div{width:10px} }");
    assert_eq!(
        sheet.rules.len(),
        1,
        "the malformed rule inside @media should be skipped, the real one after it still parses"
    );
}

#[test]
fn parses_single_rule_with_declarations() {
    let sheet = parse_stylesheet("div { color: red; width: 10px; }");
    assert_eq!(sheet.rules.len(), 1);
    let rule = &sheet.rules[0];

    assert_eq!(rule.selectors.0.len(), 1);
    assert_eq!(
        rule.selectors.0[0].0,
        vec![CompoundSelector(vec![SimpleSelector::Type("div".into())])]
    );

    assert_eq!(rule.declarations.len(), 2);
    assert_eq!(rule.declarations[0].name, "color");
    assert_eq!(rule.declarations[0].value, vec![Token::Ident("red".into())]);
    assert_eq!(rule.declarations[1].name, "width");
    assert_eq!(
        rule.declarations[1].value,
        vec![Token::Dimension(10.0, "px".into())]
    );
}

#[test]
fn parses_compound_selector_type_class_id() {
    let sheet = parse_stylesheet("div.item#main {}");
    let compound = &sheet.rules[0].selectors.0[0].0[0];
    assert_eq!(
        compound.0,
        vec![
            SimpleSelector::Type("div".into()),
            SimpleSelector::Class("item".into()),
            SimpleSelector::Id("main".into()),
        ]
    );
}

#[test]
fn parses_descendant_combinator() {
    let sheet = parse_stylesheet("div .item {}");
    let complex = &sheet.rules[0].selectors.0[0];
    assert_eq!(
        complex.0,
        vec![
            CompoundSelector(vec![SimpleSelector::Type("div".into())]),
            CompoundSelector(vec![SimpleSelector::Class("item".into())]),
        ]
    );
}

#[test]
fn parses_selector_list_with_commas() {
    let sheet = parse_stylesheet("h1, h2, .title { margin: 0; }");
    assert_eq!(sheet.rules[0].selectors.0.len(), 3);
}

#[test]
fn parses_multiple_rules() {
    let sheet = parse_stylesheet("div { color: red; } span { color: blue; }");
    assert_eq!(sheet.rules.len(), 2);
}

#[test]
fn specificity_orders_id_over_class_over_type() {
    let sheet = parse_stylesheet("div {} .foo {} #bar {}");
    let specs: Vec<_> = sheet
        .rules
        .iter()
        .map(|r| r.selectors.0[0].specificity())
        .collect();
    assert_eq!(specs, vec![(0, 0, 1), (0, 1, 0), (1, 0, 0)]);
    assert!(specs[2] > specs[1]);
    assert!(specs[1] > specs[0]);
}

#[test]
fn declaration_value_can_have_multiple_tokens() {
    let sheet = parse_stylesheet("div { margin: 10px 20px; }");
    assert_eq!(
        sheet.rules[0].declarations[0].value,
        vec![
            Token::Dimension(10.0, "px".into()),
            Token::Dimension(20.0, "px".into()),
        ]
    );
}
