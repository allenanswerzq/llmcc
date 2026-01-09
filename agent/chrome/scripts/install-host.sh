#!/bin/bash
# Install Native Messaging host for Browser Bridge
# This registers the host with Chrome/Chromium so Claude Code can find it

set -e

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BRIDGE_DIR="$(dirname "$SCRIPT_DIR")"
HOST_PATH="$BRIDGE_DIR/dist/index.js"

# Host name (must match what Claude Code looks for)
HOST_NAME="com.anthropic.browser_extension"

# Create manifest
create_manifest() {
    local target_dir="$1"
    local host_path="$2"

    mkdir -p "$target_dir"

    cat > "$target_dir/$HOST_NAME.json" << EOF
{
    "name": "$HOST_NAME",
    "description": "Browser Bridge for Claude Code",
    "path": "$host_path",
    "type": "stdio",
    "allowed_origins": [
        "chrome-extension://jmclflgclhepglnfbelejpdmelliocij/"
    ]
}
EOF
    echo "Installed manifest to: $target_dir/$HOST_NAME.json"
}

# Detect OS
case "$(uname -s)" in
    Linux*)
        # Check if running in WSL
        if grep -qi microsoft /proc/version 2>/dev/null; then
            echo "Detected: WSL"

            # For WSL, we need to install on both Linux and Windows side

            # Linux side (for Claude Code running in WSL)
            LINUX_DIR="$HOME/.config/google-chrome/NativeMessagingHosts"

            # Create wrapper script that runs node
            WRAPPER_PATH="$BRIDGE_DIR/browser-bridge-host"
            cat > "$WRAPPER_PATH" << EOF
#!/bin/bash
cd "$BRIDGE_DIR"
exec node "$HOST_PATH" "\$@"
EOF
            chmod +x "$WRAPPER_PATH"

            create_manifest "$LINUX_DIR" "$WRAPPER_PATH"

            # Also try Chromium paths
            for chrome_path in \
                "$HOME/.config/chromium/NativeMessagingHosts" \
                "$HOME/.mozilla/native-messaging-hosts" \
                "$HOME/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts" \
                "$HOME/.config/microsoft-edge/NativeMessagingHosts"
            do
                create_manifest "$chrome_path" "$WRAPPER_PATH" 2>/dev/null || true
            done

            echo ""
            echo "Note: For Windows Chrome, run install-host.ps1 from PowerShell"
        else
            echo "Detected: Linux"

            # Create wrapper script
            WRAPPER_PATH="$BRIDGE_DIR/browser-bridge-host"
            cat > "$WRAPPER_PATH" << EOF
#!/bin/bash
cd "$BRIDGE_DIR"
exec node "$HOST_PATH" "\$@"
EOF
            chmod +x "$WRAPPER_PATH"

            # Install to all common locations
            for chrome_path in \
                "$HOME/.config/google-chrome/NativeMessagingHosts" \
                "$HOME/.config/chromium/NativeMessagingHosts" \
                "$HOME/.mozilla/native-messaging-hosts" \
                "$HOME/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts" \
                "$HOME/.config/microsoft-edge/NativeMessagingHosts"
            do
                create_manifest "$chrome_path" "$WRAPPER_PATH" 2>/dev/null || true
            done
        fi
        ;;

    Darwin*)
        echo "Detected: macOS"

        # Create wrapper script
        WRAPPER_PATH="$BRIDGE_DIR/browser-bridge-host"
        cat > "$WRAPPER_PATH" << EOF
#!/bin/bash
cd "$BRIDGE_DIR"
exec node "$HOST_PATH" "\$@"
EOF
        chmod +x "$WRAPPER_PATH"

        # Install to macOS locations
        for chrome_path in \
            "$HOME/Library/Application Support/Google/Chrome/NativeMessagingHosts" \
            "$HOME/Library/Application Support/Chromium/NativeMessagingHosts" \
            "$HOME/Library/Application Support/BraveSoftware/Brave-Browser/NativeMessagingHosts" \
            "$HOME/Library/Application Support/Mozilla/NativeMessagingHosts" \
            "$HOME/Library/Application Support/Microsoft Edge/NativeMessagingHosts"
        do
            create_manifest "$chrome_path" "$WRAPPER_PATH" 2>/dev/null || true
        done
        ;;

    *)
        echo "Unsupported OS: $(uname -s)"
        exit 1
        ;;
esac

echo ""
echo "Installation complete!"
echo ""
echo "To test the installation:"
echo "  1. Build the project: npm run build"
echo "  2. Test: claude --chrome -p 'Navigate to example.com'"
