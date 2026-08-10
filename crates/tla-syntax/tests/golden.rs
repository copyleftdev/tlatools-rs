//! What every vendored specification looks like once read and written back.
//!
//! The corpus manifests in `golden/*.tsv` record *whether* each of 1,258 files
//! parses. These record *how* a handful of them are understood, in full. A
//! change in either the parser or the printer shows up here as a readable
//! diff rather than as a count that happens to stay the same.
//!
//! Regenerate with `UPDATE_GOLDEN=1 cargo test -p tla-syntax --test golden`,
//! and read the diff before committing it.

use std::path::{Path, PathBuf};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn every_specification_matches_its_golden_form() {
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let specs = workspace().join("specs");
    let golden = workspace().join("golden/fmt");

    let mut names: Vec<PathBuf> = std::fs::read_dir(&specs)
        .expect("the specs directory")
        .map(|e| e.expect("dir entry").path())
        .collect();
    names.sort();
    assert!(!names.is_empty(), "there are specifications to check");

    let mut stale = Vec::new();
    for path in names {
        let name = path
            .file_name()
            .expect("file name")
            .to_string_lossy()
            .to_string();
        let src = std::fs::read_to_string(&path).expect("readable");
        let module = tla_syntax::parse_module(&src).unwrap_or_else(|e| panic!("{name}: {e}"));
        let printed = module.to_string();

        let expected_path = golden.join(&name);
        if update {
            std::fs::write(&expected_path, &printed).expect("writable");
            continue;
        }
        let expected = std::fs::read_to_string(&expected_path)
            .unwrap_or_else(|e| panic!("{}: {e}", expected_path.display()));
        if expected != printed {
            stale.push((name, first_difference(&expected, &printed)));
        }
    }
    assert!(
        stale.is_empty(),
        "the golden form has changed:\n{}\n\
         Run `UPDATE_GOLDEN=1 cargo test -p tla-syntax --test golden` once the \
         change is intended.",
        stale
            .iter()
            .map(|(name, diff)| format!("  {name}\n{diff}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Canonical output read back has to give the same module, or the golden file
/// records something the parser cannot read.
#[test]
fn the_golden_form_is_itself_readable() {
    let golden = workspace().join("golden/fmt");
    for entry in std::fs::read_dir(&golden).expect("the golden directory") {
        let path = entry.expect("dir entry").path();
        let name = path
            .file_name()
            .expect("file name")
            .to_string_lossy()
            .to_string();
        let src = std::fs::read_to_string(&path).expect("readable");
        let once = tla_syntax::parse_module(&src).unwrap_or_else(|e| panic!("{name}: {e}"));
        let twice = tla_syntax::parse_module(&once.to_string())
            .unwrap_or_else(|e| panic!("{name}, printed a second time: {e}"));
        assert_eq!(once, twice, "{name} is not stable under printing");
    }
}

fn first_difference(expected: &str, actual: &str) -> String {
    for (line, (want, got)) in expected.lines().zip(actual.lines()).enumerate() {
        if want != got {
            return format!("    line {}\n    - {want}\n    + {got}", line + 1);
        }
    }
    format!(
        "    {} lines expected, {} produced",
        expected.lines().count(),
        actual.lines().count()
    )
}
