#!/usr/bin/env bash
# Zrzut ekranu sceny: tools/capture.sh <plik.png> <scena> [odległość m] [yaw°] [pitch°] [zakładka]
# np. tools/capture.sh /tmp/a.png apollo-11@1969-07-20T21:00 3000000
# karta z archiwum: scena craft:1977-084A albo launch:1969-059
out="$1"; scene="$2"
OPENSPACECRAFT_CAPTURE="$out" OPENSPACECRAFT_CAPTURE_AFTER="${CAPTURE_AFTER:-9}" OPENSPACECRAFT_SCENE="$scene" \
OPENSPACECRAFT_DIST="$3" OPENSPACECRAFT_YAW="$4" OPENSPACECRAFT_PITCH="$5" OPENSPACECRAFT_TAB="$6" \
  timeout 120 "${EXE:-./target/debug/OpenSpacecraft.exe}"
