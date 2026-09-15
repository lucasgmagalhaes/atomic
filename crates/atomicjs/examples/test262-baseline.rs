//! @spec atomicjs-ecma262-conformance#phase-0-harness
//! Produces a conservative Test262 baseline for the currently supported subset.

use std::path::PathBuf;

use atomicjs::test262::{discover, run_file, Outcome};

fn main() {
    let root = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("TEST262_ROOT").map(PathBuf::from))
        .unwrap_or_else(|| {
            eprintln!("usage: test262-baseline <test262/test directory> or set TEST262_ROOT");
            std::process::exit(2);
        });
    let files = discover(&root).unwrap_or_else(|error| {
        eprintln!("could not discover {}: {error}", root.display());
        std::process::exit(2);
    });
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    for path in files {
        match run_file(&path).unwrap_or_else(|error| Outcome::Failed(error.to_string())) {
            Outcome::Passed => passed += 1,
            Outcome::Skipped(_) => skipped += 1,
            Outcome::Failed(error) => {
                failed += 1;
                println!("FAIL {}: {error}", path.display());
            }
        }
    }
    println!("Test262 baseline: passed={passed} failed={failed} skipped={skipped}");
    if failed > 0 {
        std::process::exit(1);
    }
}
