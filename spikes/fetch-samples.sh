#!/usr/bin/env bash
# Downloads the CC0 RAW samples (raw.pixls.us) used by the spike 1 benchmark into ./samples.
# Works on Linux, macOS and on Windows through Git Bash. About 225 MB in total.
set -euo pipefail
cd "$(dirname "$0")"
mkdir -p samples
base=https://raw.pixls.us/data
fetch() { # <path on the server> <local name>
  [ -s "samples/$2" ] && { echo "have $2"; return; }
  echo "fetching $2"
  curl -fsSL --retry 3 -o "samples/$2" "$base/$1"   # -L: the server redirects to /download/
}
fetch "Canon/EOS%205D%20Mark%20IV/B13A0732.CR2"           B13A0732.CR2
fetch "Canon/Canon%20EOS%20R5m2/CRAW.CR3"                 canon-r5m2-CRAW.CR3
fetch "Nikon/D850/Nikon-D850-14bit-compressed.NEF"        Nikon-D850-14bit-compressed.NEF
fetch "Sony/ILCE-7RM4/DSC00396.ARW"                       DSC00396.ARW
fetch "FUJIFILM/X-T50/DSCF0120.RAF"                       DSCF0120.RAF
fetch "Panasonic/DC-S5/dc-s5_6k4k.RW2"                    dc-s5_6k4k.RW2
fetch "OLYMPUS/E-M5%20Mark%20III/PB290154.ORF"            PB290154.ORF
du -sh samples
