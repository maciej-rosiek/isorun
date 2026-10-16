#!/bin/sh
case "$LD_PRELOAD" in
  *gameoverlayrenderer.so*) unset LD_PRELOAD ;;
esac
exec "$(dirname "$(readlink -f "$0")")/isorun-electron" "$@"
