#!/usr/bin/env bash
# Downloads the CC0 RAW samples (raw.pixls.us) used by the tests and benchmarks into
# testdata/samples, verifying each against its recorded checksum (testing strategy §5).
# Works on Linux, macOS and on Windows through Git Bash. About 225 MB in total.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p testdata/samples
base=https://raw.pixls.us/data

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

fetch() { # <path on the server> <local name> <sha256>
  local local="testdata/samples/$2"
  if [ -s "$local" ]; then
    [ "$(sha256 "$local")" = "$3" ] && { echo "have $2"; return; }
    echo "$2: checksum mismatch, refetching" >&2
    rm -f "$local"
  fi
  echo "fetching $2"
  curl -fsSL --retry 3 -o "$local" "$base/$1"   # -L: the server redirects to /download/
  local got
  got="$(sha256 "$local")"
  if [ "$got" != "$3" ]; then
    echo "$2: checksum mismatch after download (got $got, expected $3)" >&2
    rm -f "$local"
    exit 1
  fi
}

fetch "Canon/EOS%205D%20Mark%20IV/B13A0732.CR2"           B13A0732.CR2                        25d928f9e9a65525ac796bdff13e93b7b36b627568d7a17338e12dc44424078a
fetch "Canon/Canon%20EOS%20R5m2/CRAW.CR3"                 canon-r5m2-CRAW.CR3                 b3c08a5c7a97299dfd58896a12b939e7b43ec78682761812524825e6e5d087be
fetch "Nikon/D850/Nikon-D850-14bit-compressed.NEF"        Nikon-D850-14bit-compressed.NEF     e54b5d4f0d5c721a90309741c60754e687750f1b2d366ac2587d870eef121126
fetch "Sony/ILCE-7RM4/DSC00396.ARW"                       DSC00396.ARW                        3421b919ee81224d59f78872ee099a021bff0e2a47ae5fbbac385b182ac7241e
fetch "FUJIFILM/X-T50/DSCF0120.RAF"                       DSCF0120.RAF                        3a8a9ac6d92274969aaa1f55ea669191b98bb8f588d00ec8ec17afd8b3963dea
fetch "Panasonic/DC-S5/dc-s5_6k4k.RW2"                    dc-s5_6k4k.RW2                      350f4ad0839383b3e7bcb0feb2dda827fc81e97ab59917df2a1d824bb979234d
fetch "OLYMPUS/E-M5%20Mark%20III/PB290154.ORF"            PB290154.ORF                        24de6ac3bd5e668ee22fd0dbf2e344608949eb85f7342f28470ce709c9d51226
fetch "Leica/M9/L1049390.DNG"                             L1049390.DNG                        d1d162fce62210b8951c862189a6098329b5163b7f7492dc397c6e53bba04239
du -sh testdata/samples
