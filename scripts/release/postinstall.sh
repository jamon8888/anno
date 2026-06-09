#!/bin/sh
# postinstall — macOS PKG post-install hook.
# Called by macOS Installer after files are placed on disk.
# Registers anno-rag as an MCP server in Claude Desktop and Claude Code.
#
# The Installer runs this script as root. We forward the setup-mcp call to
# the interactive user so that %APPDATA%/HOME/Library resolve correctly.
set -eu

BINARY=/usr/local/bin/anno-rag

# Resolve the interactive user — try $USER, $SUDO_USER, then the console user.
install_user="${USER:-}"
if [ -z "${install_user}" ] || [ "${install_user}" = "root" ]; then
    install_user="${SUDO_USER:-}"
fi
if [ -z "${install_user}" ] || [ "${install_user}" = "root" ]; then
    # Query SystemConfiguration for the currently logged-in console user.
    install_user="$(scutil <<'SCUTIL' 2>/dev/null | awk '/Name :/ && !/loginwindow/ { print $3; exit }'
show State:/Users/ConsoleUser
SCUTIL
)"
fi

if [ -z "${install_user}" ] || [ "${install_user}" = "root" ]; then
    echo "anno-rag postinstall: cannot determine interactive user — skipping MCP setup." >&2
    echo "anno-rag postinstall: run 'anno-rag setup-mcp' manually after login." >&2
    exit 0
fi

echo "anno-rag postinstall: registering MCP server for user '${install_user}'..."

# sudo -u preserves $HOME for the target user on macOS.
sudo -u "${install_user}" "${BINARY}" setup-mcp --target all --skip-models \
    && echo "anno-rag postinstall: MCP registration complete." \
    || echo "anno-rag postinstall: MCP registration returned non-zero (non-fatal)." >&2

exit 0
