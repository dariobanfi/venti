#!/bin/sh
# Writes the skeleton of a station adapter crate.
#
#   tools/new-source/new.sh <name> [<source-id>]
#
# <name> names the crate, crates/sources-<name> (package
# venti-sources-<name>): lower-case letters, digits and hyphens, beginning
# with a letter. <source-id> is the identifier region packs name the source
# by; left out, it is <name>.
#
# The skeleton compiles. Its tests fail until a fixture is recorded and the
# manifest is filled in: docs/adding-a-source.md goes through the steps.
set -eu

if [ $# -lt 1 ] || [ $# -gt 2 ]; then
    sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
fi
name=$1
id=${2:-$1}
for word in "$name" "$id"; do
    case $word in
        [a-z]*) ;;
        *) echo "$word: begin with a lower-case letter" >&2; exit 2 ;;
    esac
    case $word in
        *[!a-z0-9-]*) echo "$word: lower-case letters, digits and hyphens only" >&2; exit 2 ;;
    esac
done

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
crate="$root/crates/sources-$name"
if [ -e "$crate" ]; then
    echo "crates/sources-$name is there already. How a crate that is there joins the build:" >&2
    echo "docs/adding-a-source.md, step 1, \"A crate that is there already\"." >&2
    exit 1
fi

snake=$(printf '%s' "$name" | tr '-' '_')
type=$(printf '%s' "$name" | awk -F- '{ for (i = 1; i <= NF; i++) printf "%s%s", toupper(substr($i, 1, 1)), substr($i, 2) }')

fill() {
    mkdir -p "$(dirname "$2")"
    sed -e "s/__NAME__/$name/g" -e "s/__ID__/$id/g" -e "s/__SNAKE__/$snake/g" \
        -e "s/__TYPE__/$type/g" "$here/template/$1" > "$2"
}
fill Cargo.template.toml "$crate/Cargo.toml"
fill manifest.toml "$crate/manifest.toml"
fill src/lib.rs "$crate/src/lib.rs"
fill src/fixtures.rs "$crate/src/fixtures.rs"
fill tests/pipeline.rs "$crate/tests/pipeline.rs"
fill tests/fixtures/NOTICE.txt "$crate/tests/fixtures/NOTICE.txt"
mkdir -p "$crate/tests/golden"

cat <<END
Written: crates/sources-$name (package venti-sources-$name, type $type, source id "$id").

Three lines join it to the build:

  Cargo.toml, under [workspace.dependencies]:
      venti-sources-$name = { path = "crates/sources-$name" }
  crates/server/Cargo.toml, under [dependencies]:
      venti-sources-$name.workspace = true
  crates/server/src/adapters.rs, in station_networks():
      venti_sources_$snake::network(),

Then: docs/adding-a-source.md, from "The manifest" on. The check is
      tools/new-source/check.sh $name
END
