//! @spec atomicjs-ecma262-conformance#phase-0-harness
//! Conservative Test262 discovery and baseline classification.
//!
//! This is deliberately not a full Test262 harness yet. Unsupported metadata is
//! reported as skipped so an incomplete runner can never turn it into a pass.

use std::fs;
use std::path::{Path, PathBuf};

use crate::run_source;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disposition {
    Runnable,
    Skipped(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    Failed(String),
    Skipped(&'static str),
}

pub fn classify(source: &str) -> Disposition {
    let metadata = source
        .split_once("/*---")
        .and_then(|(_, rest)| rest.split_once("---*/"))
        .map_or("", |(metadata, _)| metadata);
    if source.contains("$DONOTEVALUATE") {
        Disposition::Skipped("parse-only test")
    } else if metadata.contains("negative:") {
        Disposition::Skipped("negative test")
    } else if metadata.contains("includes:") {
        Disposition::Skipped("harness include")
    } else if metadata.contains("[module]") {
        Disposition::Skipped("module test")
    } else if metadata.contains("[async]") || source.contains("$DONE") {
        Disposition::Skipped("async test")
    } else {
        Disposition::Runnable
    }
}

pub fn run_source_case(source: &str) -> Outcome {
    match classify(source) {
        Disposition::Skipped(reason) => Outcome::Skipped(reason),
        Disposition::Runnable => match run_source(source) {
            Ok(_) => Outcome::Passed,
            Err(error) => Outcome::Failed(error.to_string()),
        },
    }
}

pub fn discover(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    discover_into(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn discover_into(path: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            discover_into(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "js") {
            files.push(path);
        }
    }
    Ok(())
}

pub fn run_file(path: &Path) -> std::io::Result<Outcome> {
    fs::read_to_string(path).map(|source| run_source_case(&source))
}

#[cfg(test)]
mod tests {
    use super::{classify, run_source_case, Disposition, Outcome};

    #[test]
    fn runs_a_plain_positive_test() {
        assert_eq!(run_source_case("1 + 1;"), Outcome::Passed);
    }

    #[test]
    fn metadata_requiring_harness_support_is_never_counted_as_a_pass() {
        let cases = [
            (
                "/*---\nnegative:\n  phase: parse\n---*/\n$DONOTEVALUATE;",
                "parse-only test",
            ),
            ("/*---\nincludes: [assert.js]\n---*/\n1;", "harness include"),
            ("/*---\nflags: [module]\n---*/\nexport {};", "module test"),
            ("/*---\nflags: [async]\n---*/\n$DONE();", "async test"),
        ];
        for (source, reason) in cases {
            assert_eq!(classify(source), Disposition::Skipped(reason));
            assert_eq!(run_source_case(source), Outcome::Skipped(reason));
        }
    }
}
