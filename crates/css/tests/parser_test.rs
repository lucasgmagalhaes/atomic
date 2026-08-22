use css::{parse_stylesheet, CompoundSelector, SimpleSelector, Token};

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
