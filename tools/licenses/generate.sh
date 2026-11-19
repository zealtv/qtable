#!/bin/sh
# Regenerate THIRD_PARTY_NOTICES.md at the repository root from Cargo.lock.
#   tools/licenses/generate.sh           rewrite THIRD_PARTY_NOTICES.md
#   tools/licenses/generate.sh --check   fail if the committed file is out of date, or a dependency's
#                                        licence is outside the accepted list in about.toml
# Needs cargo-about (cargo install cargo-about --locked --features cli).
set -eu

cd "$(dirname "$0")/../.."
command -v cargo-about >/dev/null || { echo "generate.sh: cargo-about not found (cargo install cargo-about --locked --features cli)" >&2; exit 1; }

out=THIRD_PARTY_NOTICES.md
tmp=$(mktemp)
trap 'rm -f "${tmp:?}"' EXIT
cargo about generate --locked --workspace -c tools/licenses/about.toml -o "$tmp" tools/licenses/notices.hbs

if [ "${1:-}" = "--check" ]; then
    cmp -s "$tmp" "$out" || { echo "generate.sh: $out is out of date; run tools/licenses/generate.sh" >&2; exit 1; }
else
    cp "$tmp" "$out"
    echo "wrote $out"
fi
