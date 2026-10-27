#!/usr/bin/env bash
# Converts the fixtures and this repository's own Markdown with a spread of options, then checks
# every .docx against the Office Open XML schemas (tools/ooxml-validate, Microsoft's Open XML SDK).
# Needs the .NET 8 SDK. Usage: tools/validate-output.sh [work-dir]
set -euo pipefail
cd "$(dirname "$0")/.."
out="${1:-target/validate}"
rm -rf "$out" && mkdir -p "$out"

cargo build --locked --features mermaid --bin md2docx-cli --example fixtures
cli=target/debug/md2docx-cli
target/debug/examples/fixtures "$out" >/dev/null

i=0
for flags in \
  "" \
  "--toc --number-headings" \
  "--highlight light --page-numbers" \
  "--highlight dark --code-blocks single --landscape --page-size letter" \
  "--autolink --sup-sub --title Title --author Author"; do
  i=$((i + 1))
  for f in README.md CHANGELOG.md docs/*.md; do
    # shellcheck disable=SC2086
    "$cli" -q "$f" $flags -o "$out/opt$i-$(basename "$f" .md).docx" >/dev/null
  done
done

dotnet run --project tools/ooxml-validate -c Release -- "$out"/*.docx
