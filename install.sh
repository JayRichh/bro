#!/bin/sh
# Kete installer for macOS and Linux: installs or updates `kete`, and `bro`
# (Kete's old name, the same program) beside it. On Windows, use
# install.ps1 instead.
#
#   curl -fsSL https://raw.githubusercontent.com/JayRichh/kete/main/install.sh | sh
#
# Grabs the prebuilt binary for this platform from the latest GitHub release,
# falling back to `cargo install --git` when no release (or no matching asset)
# exists. Running it again updates in place.
#
# Environment overrides:
#   NEBULA_INSTALL_DIR   install destination (default: ~/.local/bin)
#   KETE_BRANCH          branch the source fallback builds (default: windows;
#                        BRO_BRANCH still works)
set -eu

REPO="JayRichh/kete"
BRANCH="${KETE_BRANCH:-${BRO_BRANCH:-windows}}"
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
    *) err "there's no kete build for $(uname -s)" ;;
    esac
}

install_from_release() {
    url="https://github.com/$REPO/releases/latest/download/kete-$1.tar.gz"
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    say "downloading $url"
    curl -fsSL "$url" -o "$tmp/kete.tar.gz" || return 1
    tar -xzf "$tmp/kete.tar.gz" -C "$tmp"
    mkdir -p "$INSTALL_DIR"
    install -m 755 "$tmp/kete" "$INSTALL_DIR/kete"
    install -m 755 "$tmp/kete" "$INSTALL_DIR/bro"
}

install_from_source() {
    command -v cargo >/dev/null 2>&1 ||
        err "there's no prebuilt kete for this machine and cargo isn't installed. Get Rust from https://rustup.rs and run this again"
    say "building kete from source (this takes a few minutes)..."
    cargo install --git "https://github.com/$REPO" --branch "$BRANCH" nebula --locked --force
}

main() {
    command -v curl >/dev/null 2>&1 || err "curl is required"

    installed=""
    if target=$(detect_target); then
        if install_from_release "$target"; then
            installed="$INSTALL_DIR/kete"
        else
            reason="couldn't download kete-$target from the latest release"
        fi
    else
        reason="no prebuilt kete for $(uname -s) $(uname -m)"
    fi

    if [ -z "$installed" ]; then
        say "$reason, so building from source instead"
        install_from_source
        installed="$HOME/.cargo/bin/kete"
    fi

    version=$("$installed" --version 2>/dev/null || echo kete)
    say "installed $version at $installed (and bro beside it)"

    bin_dir=$(dirname "$installed")
    case ":$PATH:" in
    *":$bin_dir:"*) ;;
    *) say "note: $bin_dir isn't on your PATH. Add: export PATH=\"$bin_dir:\$PATH\"" ;;
    esac

    # Replacing the file doesn't touch an already-running daemon: sessions keep
    # running on the old binary until it's restarted. `kete reload` restarts it
    # in place, sessions and all; a daemon from before reload existed can only
    # be restarted by `kete kill`, which stops every session. `kete upgrade`
    # handles this itself and suppresses the note via NEBULA_UPGRADE_HANDOFF.
    if [ -z "${NEBULA_UPGRADE_HANDOFF:-}" ] && pgrep -f '(kete|bro) daemon' >/dev/null 2>&1; then
        say "note: a daemon from the previous version is still running."
        say "      Run 'kete reload' to move it onto the new one (sessions keep running)."
        say "      One too old for that needs 'kete kill', which stops every session."
    fi
    say "run 'kete' to start."
}

main
