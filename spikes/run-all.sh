#!/usr/bin/env bash
# Runs every spike measure on this machine and collects the results in results-<label>/.
# Meant for Patrick's Windows and macOS machines, from the downloaded CI artifact, in Git Bash
# on Windows. Nothing is installed and nothing outside this folder is written, except that the
# viewers open a window for a few seconds at a time: do not touch the mouse or keyboard meanwhile.
#
# usage: bash run-all.sh <label>          e.g.  bash run-all.sh windows-rtx
set -u
label="${1:-local}"
out="results-$label"
mkdir -p "$out"
ext=""; case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) ext=".exe" ;; esac
bin() { for d in . target/release; do [ -x "$d/$1$ext" ] && { echo "$d/$1$ext"; return; }; done; echo "$1$ext"; }
run() { # <name> <command...>: run with a time limit, keep stdout in a file
  name="$1"; shift
  echo "== $name"
  timeout 240 "$@" > "$out/$name.txt" 2>&1; echo "   exit $?"
}

echo "machine: $label" | tee "$out/machine.txt"
run adapters "$(bin adapters)"
grep -E "\[|max buffer" "$out/adapters.txt" | head -12

# Sample RAW files (about 225 MB), if they are not there yet.
[ -d samples ] && [ "$(ls samples 2>/dev/null | grep -cE '\.(CR2|CR3|NEF|ARW|RAF|RW2|ORF)$')" -ge 6 ] || bash fetch-samples.sh

run smoke "$(bin smoke)" --adapter " "
run bench-24mp "$(bin bench)" --adapter " " --mp 24 --out "$out/bench-24mp.json"
run rawbench "$(bin rawbench)" --adapter " " --out "$out/rawbench.json"
run heavy-nikon "$(bin heavy)" --adapter " " --file samples/Nikon-D850-14bit-compressed.NEF --out "$out/heavy-nikon.json"
run presentation "$(bin presentation)" --adapter " " --file samples/Nikon-D850-14bit-compressed.NEF --out "$out/presentation.json"

# The interface toolkits: eight seconds each, on this machine's display.
for m in view grid both exact; do
  run "slint-$m" "$(bin viewer-slint)" --bench $m --secs 8 --out "$out/slint-$m.json"
done
for m in view grid both; do
  run "iced-$m" "$(bin viewer-iced)" --bench $m --secs 8 --out "$out/iced-$m.json"
done

echo
echo "Done. Send the folder $out (zip it, or copy the .json files)."
ls "$out" | head -40
