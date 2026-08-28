#!/bin/sh
set -eu

install_dir=${JJK_INSTALL_DIR:-"$HOME/.local/bin"}
target="$install_dir/jjk"

if [ -L "$target" ] || [ -f "$target" ]; then
    rm -- "$target"
    printf 'Removed %s\n' "$target"
else
    printf 'JJK executable is not installed at %s\n' "$target"
fi
printf 'Repository metadata was not removed.\n'
