#!/bin/sh
set -eu

install_dir=${JJK_INSTALL_DIR:-"$HOME/.local/bin"}
case "${1:-}" in
    -h|--help)
        cat <<'EOF'
Usage: uninstall.sh

Environment:
  JJK_INSTALL_DIR  installation directory; defaults to $HOME/.local/bin

Repository metadata is never removed.
EOF
        exit 0
        ;;
    '') ;;
    *) printf 'jjk uninstaller: unexpected argument: %s\n' "$1" >&2; exit 2 ;;
esac

target="$install_dir/jjk"

if [ -L "$target" ] || [ -f "$target" ]; then
    rm -- "$target"
    printf 'Removed %s\n' "$target"
else
    printf 'JJK executable is not installed at %s\n' "$target"
fi
printf 'Repository metadata was not removed.\n'
