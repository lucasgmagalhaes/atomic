pub mod cascade;
pub mod lexer;
pub mod parser;

pub use cascade::{
    build_selector_index, matching_declarations, matching_declarations_indexed, selector_matches,
    ElementSnapshot, MatchedDeclarations, SelectorIndex,
};
pub use lexer::{tokenize, Lexer, Token};
pub use parser::{
    parse_inline_style, parse_stylesheet, AttributeMatch, AttributeSelector, Combinator,
    ComplexSelector, CompoundSelector, Declaration, FontFaceRule, ImportRule, MediaFeature,
    MediaQuery, NthFormula, PseudoClass, Rule, SelectorList, SimpleSelector, Stylesheet,
};
