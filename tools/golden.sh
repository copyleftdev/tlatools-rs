#!/bin/sh
# Record, or check, exactly how this parser reads every specification we can
# find.
#
# The corpora are large and belong to other projects, so they are not vendored;
# what is committed is the verdict for each file. A behaviour change shows up
# as a diff naming the files it changed, which a pass/fail count cannot.
#
#     tools/golden.sh          regenerate golden/*.tsv
#     tools/golden.sh --check  fail if anything differs
#
# Corpus locations come from the environment, so CI and a workstation can put
# them wherever suits:
#
#     TLA_EXAMPLES   github.com/tlaplus/Examples
#     TLA_COMMUNITY  github.com/tlaplus/CommunityModules
#     TLA_TESTS      github.com/tlaplus/tlaplus
#
# CI pins each to a commit, because a check that fails when somebody else
# commits is not a check. To reproduce what CI sees:
#
#     git -C Examples          checkout 52c4c651022b2d50bc9ca2aca1bd53277d4332c6
#     git -C CommunityModules  checkout a8068a4c21ed76b339b9a2aa6de69d78f64f6422
#     git -C tlaplus           checkout 30cc3601321c3fc02e044d0ecb5c58d8921e18df
#
# TLA_TESTS points at the tlaplus repository root; only its test-model and test
# directories hold .tla files.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
tlatools="$root/target/release/tlatools"
[ -x "$tlatools" ] || cargo build --release --quiet --manifest-path "$root/Cargo.toml"

check=${1:-}
status=0

record () { # $1 = golden name, $2 = corpus root
  name=$1
  corpus=$2
  golden="$root/golden/$name.tsv"
  if [ -z "$corpus" ] || [ ! -d "$corpus" ]; then
    echo "skip  $name (not present)" >&2
    return 0
  fi
  # Paths are relative to the corpus root and sorted, so the file is stable
  # wherever the corpus was checked out.
  produced=$(cd "$corpus" && find . -name '*.tla' | LC_ALL=C sort | xargs "$tlatools" parse || true)
  if [ "$check" = "--check" ]; then
    if printf '%s\n' "$produced" | diff -u "$golden" - >/dev/null 2>&1; then
      echo "ok    $name"
    else
      echo "DIFF  $name"
      printf '%s\n' "$produced" | diff -u "$golden" - | head -40 || true
      status=1
    fi
  else
    printf '%s\n' "$produced" > "$golden"
    echo "wrote $name ($(wc -l < "$golden") files)"
  fi
}

record tlaplus-examples   "${TLA_EXAMPLES:-}"
record community-modules  "${TLA_COMMUNITY:-}"
record tlaplus-tests      "${TLA_TESTS:-}"
exit $status
