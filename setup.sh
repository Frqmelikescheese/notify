#!/bin/bash
set -e

NOTIFY_DIR="$(cd "$(dirname "$0")" && pwd)"
THEMES_DIR="$NOTIFY_DIR/themes"
CONFIG_FILE="$NOTIFY_DIR/config.toml"
STYLE_FILE="$NOTIFY_DIR/style.css"

# ─── Colors ─────────────────────────────────────────────
BOLD='\033[1m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
MAGENTA='\033[0;35m'
NC='\033[0m'

echo -e "${BOLD}${MAGENTA}"
echo "  ╻ ╻┏┓ ┏┓╻┏┳┓╻ ╻┏ ┓"
echo "  ┃┏┛┣┻┓┃┗┫┃┃┃┃ ┃┃┃┃"
echo "  ┗┛ ┗━┛╹ ╹╹ ╹┗━┛┗┻┛"
echo -e "${NC}"
echo -e "${BOLD}Notification Daemon Setup${NC}"
echo ""

# ─── Dependency Check ───────────────────────────────────
echo -e "${CYAN}Checking dependencies...${NC}"
MISSING=""
for cmd in cargo dbus-send notify-send; do
    if ! command -v "$cmd" &>/dev/null; then
        MISSING="$MISSING $cmd"
    fi
done
if [ -n "$MISSING" ]; then
    echo -e "${YELLOW}Missing:$MISSING${NC}"
    echo "  - cargo:  https://rustup.rs/"
    echo "  - notify-send: install libnotify (apt install libnotify-bin / pacman -S libnotify)"
    echo "  - dbus-send: install dbus (usually pre-installed)"
    echo ""
    read -rp "Continue anyway? [y/N] " ans
    [[ "$ans" =~ ^[yY] ]] || exit 1
fi
echo -e "${GREEN}✓${NC} Dependencies OK"
echo ""

# ─── Theme Selection ─────────────────────────────────────
echo -e "${CYAN}Available Themes:${NC}"
echo ""

themes=()
theme_names=()
while IFS= read -r f; do
    name=$(basename "$f" .css)
    themes+=("$f")
    theme_names+=("$name")
done < <(find "$THEMES_DIR" -name '*.css' -type f 2>/dev/null | sort)

if [ ${#themes[@]} -eq 0 ]; then
    echo -e "${YELLOW}No themes found in themes/. Using default.${NC}"
    SELECTED_THEME=""
else
    # Show theme descriptions (keyed by filename)
    declare -A desc_map=(
        ["catppuccin-mocha"]="Catppuccin Mocha (warm dark purple)"
        ["catppuccin-latte"]="Catppuccin Latte (warm light)"
        ["discord-dark"]="Discord-inspired dark theme"
        ["nord"]="Nord (cool arctic dark)"
        ["gruvbox"]="Gruvbox (retro warm dark)"
        ["liquid-glass"]="Liquid Glass (translucent frost)"
        ["tokyo-night"]="Tokyo Night (deep blue dark)"
    )
    echo "  ┌─────┬─────────────────────────────┬──────────────────────────────────────┐"
    echo "  │  #  │ Theme                       │ Description                          │"
    echo "  ├─────┼─────────────────────────────┼──────────────────────────────────────┤"
    for i in "${!theme_names[@]}"; do
        name="${theme_names[$i]}"
        desc="${desc_map[$name]:-Custom theme}"
        printf "  │ %3s │ %-27s │ %-36s │\n" "$((i+1))" "${name//-/ • }" "$desc"
    done
    echo "  └─────┴─────────────────────────────┴──────────────────────────────────────┘"
    echo ""

    # Prompt for selection
    DEFAULT=1
    read -rp "Select theme [1-${#themes[@]}] (default: $DEFAULT): " choice
    choice="${choice:-$DEFAULT}"
    if [[ "$choice" =~ ^[0-9]+$ ]] && [ "$choice" -ge 1 ] && [ "$choice" -le "${#themes[@]}" ]; then
        idx=$((choice - 1))
        SELECTED_THEME="${themes[$idx]}"
        echo -e "${GREEN}→ Selected: ${theme_names[$idx]}${NC}"
    else
        echo -e "${YELLOW}Invalid choice, using default.${NC}"
        SELECTED_THEME=""
    fi
fi
echo ""

# ─── Install Theme ───────────────────────────────────────
if [ -n "$SELECTED_THEME" ]; then
    echo -e "${CYAN}Installing theme...${NC}"
    cp "$SELECTED_THEME" "$STYLE_FILE"
    echo -e "${GREEN}✓${NC} Installed $(basename "$SELECTED_THEME") as style.css"
fi

# ─── Auto-Start Setup ────────────────────────────────────
echo ""
echo -e "${CYAN}Auto-start Setup:${NC}"
echo "  1) Hyprland  (add to hyprland.conf)"
echo "  2) KDE / XDG  (install autostart .desktop file)"
echo "  3) Skip"
read -rp "Choose [1-3] (default: 3): " autostart_choice
autostart_choice="${autostart_choice:-3}"

if [ "$autostart_choice" = "1" ]; then
    HYPR_CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}/hypr/hyprland.conf"
    HYPR_DIR="$(dirname "$HYPR_CONFIG")"
    if [ ! -d "$HYPR_DIR" ]; then
        echo -e "${YELLOW}Hyprland config directory not found. Creating...${NC}"
        mkdir -p "$HYPR_DIR"
    fi

    AUTOSTART_LINE="exec-once = $NOTIFY_DIR/target/release/notify"
    if [ -f "$HYPR_CONFIG" ] && grep -q "notify" "$HYPR_CONFIG"; then
        echo -e "${YELLOW}notify already in hyprland.conf, skipping.${NC}"
    else
        echo "" >> "$HYPR_CONFIG"
        echo "# Auto-start notify notification daemon" >> "$HYPR_CONFIG"
        echo "$AUTOSTART_LINE" >> "$HYPR_CONFIG"
        echo -e "${GREEN}✓${NC} Added to $HYPR_CONFIG"
    fi
elif [ "$autostart_choice" = "2" ]; then
    AUTOSTART_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"
    mkdir -p "$AUTOSTART_DIR"
    DESKTOP_FILE="$AUTOSTART_DIR/notify.desktop"
    cat > "$DESKTOP_FILE" << EOF
[Desktop Entry]
Type=Application
Name=Notify
Comment=Notification Daemon
Exec=$NOTIFY_DIR/target/release/notify
Terminal=false
NoDisplay=true
X-GNOME-Autostart-enabled=true
EOF
    echo -e "${GREEN}✓${NC} Installed $DESKTOP_FILE"
fi

# ─── Blocking Process Check ──────────────────────────────
echo ""
echo -e "${CYAN}Checking for conflicting notification services...${NC}"
BLOCKER_PID=$(dbus-send --session --dest=org.freedesktop.DBus --print-reply \
    /org/freedesktop/DBus org.freedesktop.DBus.GetConnectionUnixProcessID \
    string:org.freedesktop.Notifications 2>/dev/null | awk 'END{print $NF}')

if [ -n "$BLOCKER_PID" ] && [ "$BLOCKER_PID" -ne 0 ] 2>/dev/null; then
    BLOCKER_NAME=$(ps -p "$BLOCKER_PID" -o comm= 2>/dev/null || echo "unknown")
    echo -e "${YELLOW}⚠  DBus name taken by PID $BLOCKER_PID ($BLOCKER_NAME)${NC}"
    read -rp "Kill it? [y/N] " kill_it
    if [[ "$kill_it" =~ ^[yY] ]]; then
        kill "$BLOCKER_PID" 2>/dev/null && echo -e "${GREEN}✓${NC} Killed $BLOCKER_PID" || echo -e "${YELLOW}Failed to kill${NC}"
    fi
else
    echo -e "${GREEN}✓${NC} No conflicts"
fi

# ─── Build ────────────────────────────────────────────────
echo ""
echo -e "${CYAN}Building notify...${NC}"
cargo build --release 2>&1 | tail -1
echo -e "${GREEN}✓${NC} Build complete"
echo ""

# ─── Summary ──────────────────────────────────────────────
echo -e "${BOLD}${GREEN}── Setup Complete ──${NC}"
echo ""
echo -e "  ${BOLD}Run:${NC}  ./target/release/notify &"
echo -e "  ${BOLD}Test:${NC} notify-send 'Hello' 'it works!' --icon firefox"
echo ""
echo -e "  ${BOLD}Config:${NC}  $CONFIG_FILE"
echo -e "  ${BOLD}Style:${NC}   $STYLE_FILE"
echo ""
echo -e "  ${BOLD}To switch themes later:${NC}"
echo "    ./setup.sh"
echo "    # or manually: cp themes/catppuccin-mocha.css style.css"
echo ""
echo -e "  ${BOLD}Troubleshooting:${NC}"
echo "    If notifications don't show, check no other service owns the name:"
echo "    dbus-send --session --dest=org.freedesktop.DBus --print-reply \\"
echo "      /org/freedesktop/DBus org.freedesktop.DBus.ListNames"
echo ""
