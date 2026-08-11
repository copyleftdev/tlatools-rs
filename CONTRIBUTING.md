# Contributing

Thanks for looking. This is a small project with a clear shape, so here is what
is most useful and how to check your work.

## The most useful contribution

**A TLA+ file this reads wrongly.** That is worth more than anything else,
because the whole project is measured against real specifications rather than
against opinions about the grammar. If you have a module that fails to parse,
parses into something odd, or evaluates to the wrong answer, open an issue with
the file. Small is good, but a large real one is fine too.

If you can, say what SANY or TLC does with the same file — the difference is
usually the whole bug report.

## Running things

```console
$ cargo test
$ cargo clippy --all-targets
```

Both must be clean. Clippy runs at `pedantic`; if a lint is wrong for the case,
`#[expect(..., reason = "...")]` with a real reason is fine, `#[allow]` without
one is not.

## The corpora

The parser is measured against every public TLA+ specification we could find.
They are not vendored — they belong to other projects — so point at your own
checkouts:

```console
$ git clone --depth 1 https://github.com/tlaplus/Examples
$ git clone --depth 1 https://github.com/tlaplus/CommunityModules
$ git clone --depth 1 https://github.com/tlaplus/tlaplus

$ cargo run --release --example audit -p tla-syntax -- $(find Examples -name '*.tla')
parsed 418 / 420
...
```

`audit` groups every failure by reason and shows the offending line, which is
the fastest way to find what is missing.

## Golden files

`golden/*.tsv` records how each of 1,268 corpus files is read. If your change
affects any of them, the diff will say which — that is the point of it, and a
diff of a dozen files with a good reason is welcome.

```console
$ TLA_EXAMPLES=... TLA_COMMUNITY=... TLA_TESTS=... tools/golden.sh --check
$ TLA_EXAMPLES=... TLA_COMMUNITY=... TLA_TESTS=... tools/golden.sh   # regenerate
```

`golden/fmt/` holds the full canonical form of the vendored specifications:

```console
$ UPDATE_GOLDEN=1 cargo test -p tla-syntax --test golden
```

**Read the diff before committing it.** A golden file that changed for a reason
you cannot state is a bug you have not found yet — that has happened here, and
the manifests caught a change that all the unit tests missed.

## Mutation testing

CI checks that changed code is covered, using `cargo mutants --in-diff`. To run
it yourself, bound it — it will otherwise use every core you have for twenty
minutes:

```console
$ CARGO_MUTANTS_JOBS=4 nice -n 19 cargo mutants
```

A surviving mutant usually means one of two things: a test is missing, or the
code is doing something nothing depends on. Both are worth knowing, and the
second is worth deleting.

## Style

Code should read like the code around it. Comments explain *why*, not what —
the what is in the code. Where a constant was chosen by measurement, the
measurement belongs in the comment (see `DEFAULT_NESTING_LIMIT`).

## Releasing

Maintainers: bump `version` in the workspace `Cargo.toml`, update
`CHANGELOG.md`, then tag.

```console
$ git tag v0.2.0 && git push --tags
```

CI verifies, builds binaries for five platforms, cuts the GitHub release and
publishes the crates in dependency order.
