//! @spec atomicjs-ecma262-conformance#phase-0-harness
use atomicjs::test262::{classify, Disposition};

#[test]
fn test262_harness_leaves_unsupported_features_visible() {
    assert_eq!(
        classify("/*---\nflags: [module]\n---*/\nexport {};"),
        Disposition::Skipped("module test")
    );
}
