#!/bin/sh
# bro installer: installs or updates the `bro` binary on macOS and Linux.
# On Windows, use install.ps1 instead.
#
#   curl -fsSL https://raw.githubusercontent.com/JayRichh/bro/main/install.sh | sh
#
# Grabs the prebuilt binary for this platform from the latest GitHub release,
# falling back to `cargo install --git` when no release (or no matching asset)
# exists. Running it again updates in place.
#
# Environment overrides:
#   NEBULA_INSTALL_DIR   install destination (default: ~/.local/bin)
#   BRO_BRANCH           branch the source fallback builds (default: windows)
set -eu

REPO="JayRichh/bro"
BRANCH="${BRO_BRANCH:-windows}"
INSTALL_DIR="${NEBULA_INSTALL_DIR:-$HOME/.local/bin}"

say() { printf '%s\n' "$*"; }
err() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

detect_target() {
    case "$(uname -s)" in
    Darwin)
        case "$(uname -m)" in
        arm64) echo aarch64-apple-darwin ;;
        x86_64) echo x86_64-apple-darwin ;;
        *) return 1 ;;
        esac
        ;;
    Linux)
        case "$(uname -m)" in
        x86_64) echo x86_64-unknown-linux-musl ;;
        aarch64 | arm64) echo aarch64-unknown-linux-musl ;;
        *) return 1 ;;
        esac
        ;;
    MINGW* | MSYS* | CYGWIN*)
        err "on Windows, install with PowerShell: irm https://raw.githubusercontent.com/$REPO/main/install.ps1 | iex"
        ;;
    *) err "no bro build for $(uname -s)" ;;
    esac
}

install_from_release() {
    url="https://github.com/$REPO/releases/latest/download/bro-$1.tar.gz"
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    say "downloading $url"
    curl -fsSL "$url" -o "$tmp/bro.tar.gz" || return 1
    tar -xzf "$tmp/bro.tar.gz" -C "$tmp"
    mkdir -p "$INSTALL_DIR"
    install -m 755 "$tmp/bro" "$INSTALL_DIR/bro"
}

install_from_source() {
    command -v cargo >/dev/null 2>&1 ||
        err "no prebuilt binary available and cargo is not installed. Get Rust from https://rustup.rs and re-run"
    say "building from source (this takes a few minutes)..."
    cargo install --git "https://github.com/$REPO" --branch "$BRANCH" nebula --locked --force
}

main() {
    command -v curl >/dev/null 2>&1 || err "curl is required"

    installed=""
    if target=$(detect_target); then
        if install_from_release "$target"; then
            installed="$INSTALL_DIR/bro"
        else
            reason="couldn't download bro-$target from the latest release"
        fi
    else
        reason="no prebuilt binary for $(uname -s) $(uname -m)"
    fi

    if [ -z "$installed" ]; then
        say "$reason, building from source instead"
        install_from_source
        installed="$HOME/.cargo/bin/bro"
    fi

    version=$("$installed" --version 2>/dev/null || echo bro)
    say "installed $version -> $installed"

    bin_dir=$(dirname "$installed")
    case ":$PATH:" in
    *":$bin_dir:"*) ;;
    *) say "note: $bin_dir is not on your PATH. Add: export PATH=\"$bin_dir:\$PATH\"" ;;
    esac

    # Replacing the file doesn't touch an already-running daemon: sessions keep
    # running on the old binary until it's restarted. `bro reload` restarts it
    # in place, sessions and all; a daemon from before reload existed can only
    # be restarted by `bro kill`, which stops every session. `bro upgrade`
    # handles this itself and suppresses the note via NEBULA_UPGRADE_HANDOFF.
    if [ -z "${NEBULA_UPGRADE_HANDOFF:-}" ] && pgrep -f 'bro daemon' >/dev/null 2>&1; then
        say "note: a bro daemon from the previous version is still running."
        say "      run 'bro reload' to move it onto the new one (sessions keep running);"
        say "      a daemon too old for that needs 'bro kill' (stops all sessions)."
    fi
}

main
