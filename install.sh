#!/usr/bin/env bash
# Per-user installer for GitHub release binaries. Compatible with macOS Bash 3.2.
set -euo pipefail

fail() { printf 'Error: %s\n' "$*" >&2; exit 1; }
version=''
bin_dir="${HOME:?HOME must be set}/.local/bin"
while [ "$#" -gt 0 ]; do
    case "$1" in
        --version|--bin-dir)
            [ "$#" -ge 2 ] || fail "$1 requires a value"
            if [ "$1" = --version ]; then version="${2#v}"; else bin_dir="$2"; fi
            shift 2 ;;
        --help|-h)
            printf 'Usage: bash install.sh [--version VERSION] [--bin-dir ABSOLUTE_PATH]\n'
            exit 0 ;;
        *) fail "Unknown option: $1" ;;
    esac
done
case "$bin_dir" in /*) ;; *) fail '--bin-dir must be an absolute path' ;; esac

os=$(uname -s)
arch=$(uname -m)
case "$os:$arch" in
    Linux:x86_64|Linux:amd64) target=x86_64-unknown-linux-gnu ;;
    Darwin:arm64|Darwin:aarch64) target=aarch64-apple-darwin ;;
    Darwin:x86_64)
        # Prefer the native release when invoked from a Rosetta terminal.
        if [ "$(sysctl -in sysctl.proc_translated 2>/dev/null || true)" = 1 ]; then
            target=aarch64-apple-darwin
        else
            target=x86_64-apple-darwin
        fi ;;
    *) fail "Unsupported platform: $os $arch. Releases support Linux x86-64 and Intel/Apple Silicon macOS." ;;
esac
if [ "$os" = Linux ]; then
    libc=$(getconf GNU_LIBC_VERSION 2>/dev/null) || fail 'Linux releases require glibc 2.39 or newer; musl is not supported'
    [[ "$libc" =~ ^glibc\ ([0-9]+)\.([0-9]+)$ ]] || fail "Unsupported Linux libc: $libc"
    if [ "${BASH_REMATCH[1]}" -lt 2 ] || { [ "${BASH_REMATCH[1]}" -eq 2 ] && [ "${BASH_REMATCH[2]}" -lt 39 ]; }; then
        fail "Linux releases require glibc 2.39 or newer; found $libc"
    fi
fi
for tool in curl tar mktemp; do command -v "$tool" >/dev/null || fail "Required command not found: $tool"; done
if command -v sha256sum >/dev/null; then
    hash_command=(sha256sum)
elif command -v shasum >/dev/null; then
    hash_command=(shasum -a 256)
else
    fail 'SHA-256 verification requires sha256sum or shasum'
fi

repo=https://github.com/MSpiechowicz/useful-timer
if [ -z "$version" ]; then
    latest=$(curl --proto '=https' --tlsv1.2 -fsSL -o /dev/null -w '%{url_effective}' "$repo/releases/latest") || fail 'Cannot resolve the latest release. Check your connection and whether a release has been published.'
    version="${latest##*/v}"
fi
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "Invalid release version: $version"
asset="useful-timer-$version-$target.tar.gz"
work=$(mktemp -d)
staged=''
trap 'rm -rf "$work"; if [ -n "$staged" ]; then rm -f "$staged"; fi' EXIT
printf 'Downloading Useful Timer %s (%s)…\n' "$version" "$target"
for file in "$asset" "$asset.sha256"; do
    curl --proto '=https' --tlsv1.2 -fsSL "$repo/releases/download/v$version/$file" -o "$work/$file" || fail "Cannot download $file. This release must include the archive and its checksum."
done
read -r expected filename < "$work/$asset.sha256" || fail 'Invalid checksum file'
[[ "$expected" =~ ^[0-9a-fA-F]{64}$ ]] && [ "$filename" = "$asset" ] || fail 'Invalid checksum file'
actual=$("${hash_command[@]}" "$work/$asset")
[ "$expected" = "${actual%% *}" ] || fail 'SHA-256 checksum mismatch; nothing was installed'
# Extract only known regular files, never arbitrary archive paths or symlinks.
mkdir "$work/package"
for file in useful-timer LICENSE README.md useful-timer.png; do
    tar -xOzf "$work/$asset" "$file" > "$work/package/$file"
    [ -s "$work/package/$file" ] || fail "Missing or empty archive member: $file"
done

mkdir -p "$bin_dir"
if [ "$os" = Darwin ]; then
    app="$HOME/Applications/Useful Timer.app/Contents"
    mkdir -p "$app/MacOS" "$app/Resources"
    destination="$app/MacOS/useful-timer"
    cp "$work/package/LICENSE" "$work/package/README.md" "$app/Resources/"
    cat > "$work/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Useful Timer</string>
<key>CFBundleDisplayName</key><string>Useful Timer</string>
<key>CFBundleIdentifier</key><string>io.github.MSpiechowicz.useful-timer</string>
<key>CFBundleExecutable</key><string>useful-timer</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>CFBundleVersion</key><string>$version</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
    cp "$work/Info.plist" "$app/Info.plist"
else
    destination="$bin_dir/useful-timer"
    data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
    case "$data_dir" in /*) ;; *) data_dir="$HOME/.local/share" ;; esac
    mkdir -p "$data_dir/useful-timer" "$data_dir/applications"
    cp "$work/package/LICENSE" "$work/package/README.md" "$work/package/useful-timer.png" "$data_dir/useful-timer/"
    # Desktop Entry string escaping, followed by quoted Exec argument escaping.
    desktop_path=${destination//\\/\\\\}
    desktop_path=${desktop_path//\$/\\\$}
    desktop_path=${desktop_path//\`/\\\`}
    desktop_path=${desktop_path//\"/\\\"}
    desktop_path=${desktop_path//%/%%}
    desktop_path=${desktop_path//\\/\\\\}
    icon_path="$data_dir/useful-timer/useful-timer.png"
    icon_path=${icon_path//\\/\\\\}
    cat > "$data_dir/applications/useful-timer.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Useful Timer
Comment=Animated desktop countdown timers
Exec="$desktop_path"
Icon=$icon_path
Terminal=false
Categories=Utility;Clock;
DESKTOP
fi
# A sibling temporary file makes replacement atomic, including while an old binary runs.
staged=$(mktemp "${destination%/*}/.useful-timer.XXXXXX")
cp "$work/package/useful-timer" "$staged"
chmod 755 "$staged"
mv -f "$staged" "$destination"
staged=''
if [ "$os" = Darwin ]; then
    ln -sfn "$destination" "$bin_dir/useful-timer"
    printf 'Installed: %s\nOpen Useful Timer from ~/Applications. Unsigned releases may require approval in System Settings > Privacy & Security.\n' "$HOME/Applications/Useful Timer.app"
else
    printf 'Installed: %s\nOpen Useful Timer from your applications menu. Linux requires glibc 2.39+, ALSA, X11/XWayland, and OpenGL runtime libraries.\n' "$destination"
fi
case ":$PATH:" in
    *":$bin_dir:"*) printf 'Or run: useful-timer\n' ;;
    *) printf 'To run from a terminal, add this to your shell profile:\n  export PATH=%q:"$PATH"\n' "$bin_dir" ;;
esac
