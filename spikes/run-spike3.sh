#!/usr/bin/env bash
# Spike 3 on this machine: the SQLite catalogue, thumbnails and workspace of sidecars, then the
# grid. Needs the RAW previews written by rawbench in samples/previews (run run-all.sh first, or
# rawbench). Writes about 3.5 GB under $AUR_DATA (default: the system temporary folder), and the
# results in results3-<label>/. The grid opens a window for about a minute: do not touch the mouse
# or keyboard meanwhile.
#
# usage: bash run-spike3.sh <label>
set -u
label="${1:-local}"; out="results3-$label"; mkdir -p "$out"
ext=""; case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) ext=".exe" ;; esac
bin() { for d in . target/release; do [ -x "$d/$1$ext" ] && { echo "$d/$1$ext"; return; }; done; echo "$1$ext"; }
run() { name="$1"; shift; echo "== $name"; timeout 1500 "$@" > "$out/$name.txt" 2>&1; echo "   exit $?"; tail -3 "$out/$name.txt"; }
[ -d samples/previews ] || { echo "samples/previews is missing: run rawbench first (bash run-all.sh)"; exit 1; }
run build_db "$(bin build_db)"
run queries "$(bin query_bench)" --out "$out/queries.json"
run thumbnails "$(bin thumbgen)" --lean --out "$out/thumbnails.json"
run workspace-write "$(bin workspace)" --write --edit --out "$out/workspace-write.json"
run workspace-rebuild "$(bin workspace)" --rebuild --out "$out/workspace-rebuild.json"
run exifcost "$(bin exifcost)" samples
run grid "$(bin viewer-catalogue)" --secs 10 --out "$out/grid.json"
echo; echo "Done. Send the folder $out."
