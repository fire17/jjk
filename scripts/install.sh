#!/bin/sh
set -eu

repository=${JJK_REPOSITORY:-fire17/jjk}
install_dir=${JJK_INSTALL_DIR:-"$HOME/.local/bin"}
version=${JJK_VERSION:-}
case "${1:-}" in
    -h|--help)
        cat <<'EOF'
Usage: install.sh

Environment:
  JJK_VERSION      release tag, for example v0.1.0; defaults to latest
  JJK_INSTALL_DIR  installation directory; defaults to $HOME/.local/bin
  JJK_REPOSITORY   GitHub owner/repository; defaults to fire17/jjk
EOF
        exit 0
        ;;
    '') ;;
    *) printf 'jjk installer: unexpected argument: %s\n' "$1" >&2; exit 2 ;;
esac


need() {
    command -v "$1" >/dev/null 2>&1 || {
        printf 'jjk installer: required command not found: %s\n' "$1" >&2
        exit 1
    }
}

need curl
need tar
need uname
need mktemp
need install

if [ -z "$version" ]; then
    version=$(curl -fsSL "https://api.github.com/repos/$repository/releases/latest" |
        sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' |
        head -n 1)
fi
numeric_version=${version#v}
case "$version:$numeric_version" in
    v[0-9]*.[0-9]*.[0-9]*:[0-9]*.[0-9]*.[0-9]*) ;;
    *) printf 'jjk installer: expected JJK_VERSION=vMAJOR.MINOR.PATCH, got %s\n' "$version" >&2; exit 1 ;;
esac
case "$numeric_version" in
    *[!0-9.]*|*.*.*.*|.*|*.|*..*) printf 'jjk installer: expected JJK_VERSION=vMAJOR.MINOR.PATCH, got %s\n' "$version" >&2; exit 1 ;;
esac

case "$(uname -s)" in
    Darwin) os=darwin ;;
    Linux) os=linux ;;
    *) printf 'jjk installer: unsupported operating system: %s\n' "$(uname -s)" >&2; exit 1 ;;
esac
case "$(uname -m)" in
    x86_64|amd64) arch=x64 ;;
    arm64|aarch64) arch=arm64 ;;
    *) printf 'jjk installer: unsupported architecture: %s\n' "$(uname -m)" >&2; exit 1 ;;
esac

asset="jjk-${version}-${os}-${arch}.tar.gz"
base="https://github.com/$repository/releases/download/$version"
tmp=$(mktemp -d "${TMPDIR:-/tmp}/jjk-install.XXXXXX")
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

curl -fL "$base/$asset" -o "$tmp/$asset"
curl -fL "$base/$asset.sha256" -o "$tmp/$asset.sha256"

if command -v sha256sum >/dev/null 2>&1; then
    (cd "$tmp" && sha256sum -c "$asset.sha256")
elif command -v shasum >/dev/null 2>&1; then
    expected=$(sed 's/[[:space:]].*$//' "$tmp/$asset.sha256")
    actual=$(shasum -a 256 "$tmp/$asset" | sed 's/[[:space:]].*$//')
    test "$actual" = "$expected" || {
        printf 'jjk installer: checksum verification failed\n' >&2
        exit 1
    }
else
    printf 'jjk installer: sha256sum or shasum is required\n' >&2
    exit 1
fi

tar -xzf "$tmp/$asset" -C "$tmp"
source_binary="$tmp/jjk-${version}-${os}-${arch}/jjk"
test -f "$source_binary" || {
    printf 'jjk installer: archive does not contain expected jjk binary\n' >&2
    exit 1
}
mkdir -p "$install_dir"
install -m 0755 "$source_binary" "$install_dir/jjk"
printf 'Installed JJK %s to %s/jjk\n' "$version" "$install_dir"
case ":${PATH:-}:" in
    *:"$install_dir":*) ;;
    *) printf 'Add %s to PATH to invoke jjk.\n' "$install_dir" ;;
esac
