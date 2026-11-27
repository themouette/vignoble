#!/usr/bin/env bash
set -e

# Installation script for the vignoble suite (vignoble, sarment, cep)
# Usage: curl -fsSL https://raw.githubusercontent.com/themouette/vignoble/main/install.sh | bash

REPO="themouette/vignoble"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() {
  echo -e "${BLUE}==>${NC} $1"
}

log_success() {
  echo -e "${GREEN}✓${NC} $1"
}

log_error() {
  echo -e "${RED}✗${NC} $1"
}

log_warning() {
  echo -e "${YELLOW}!${NC} $1"
}

show_help() {
  cat << EOF
vignoble installer

USAGE:
    curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | bash
    curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | bash -s -- [OPTIONS]

OPTIONS:
    --version <VERSION>        Version to install (default: latest)
                               Examples: v0.1.0, latest

    --destination <PATH>       Custom installation directory
                               Overrides --global

    --global                   Install to /usr/local/bin (requires sudo)
                               Default is ~/.local/bin (no sudo required)

    --shell-integration        Add shell integration (cep cd wrapper + completions)
                               Adds 'eval "\$(vignoble shell-init --with-completion)"'
                               to your shell configuration file

    --help                     Show this help message

SUITE TOOLS:
    Installs the 'vignoble' binary and runs 'vignoble postinstall' to link
    the suite CLIs ('sarment' and 'cep') in the installation directory.

EOF
  exit 0
}

detect_platform() {
  local os=""
  local arch=""

  case "$OSTYPE" in
    darwin*)
      os="macos"
      ;;
    linux*)
      os="linux"
      ;;
    *)
      log_error "Unsupported OS: $OSTYPE"
      exit 1
      ;;
  esac

  case "$(uname -m)" in
    x86_64|amd64)
      arch="x86_64"
      ;;
    aarch64|arm64)
      arch="aarch64"
      ;;
    *)
      log_error "Unsupported architecture: $(uname -m)"
      exit 1
      ;;
  esac

  echo "${os}-${arch}"
}

get_latest_version() {
  curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name":' \
    | sed -E 's/.*"([^"]+)".*/\1/'
}

check_path() {
  local dir="$1"
  if echo "$PATH" | tr ':' '\n' | grep -q "^${dir}$"; then
    return 0
  else
    return 1
  fi
}

detect_shell_rc() {
  local shell_name
  shell_name=$(basename "${SHELL:-}")

  case "$shell_name" in
    bash)
      echo "$HOME/.bashrc"
      ;;
    zsh)
      echo "$HOME/.zshrc"
      ;;
    fish)
      echo "$HOME/.config/fish/config.fish"
      ;;
    *)
      echo ""
      ;;
  esac
}

add_shell_init() {
  local rc_file="$1"

  if [ -z "$rc_file" ]; then
    log_warning "Could not detect shell RC file"
    echo ""
    echo "To enable shell integration (cep cd + tab completion), add this to your shell configuration:"
    echo '  eval "$(vignoble shell-init --with-completion)"'
    return 1
  fi

  if [ -f "$rc_file" ] && grep -q "vignoble shell-init" "$rc_file" 2>/dev/null; then
    log_info "Shell integration already configured in $rc_file"
    return 0
  fi

  mkdir -p "$(dirname "$rc_file")" 2>/dev/null

  {
    echo ""
    echo "# vignoble shell integration (cep cd + tab completion)"
    echo 'eval "$(vignoble shell-init --with-completion)"'
  } >> "$rc_file"

  log_success "Shell integration added to $rc_file"
  return 0
}

main() {
  local platform
  platform=$(detect_platform)
  local version="latest"
  local install_dir="$HOME/.local/bin"
  local shell_integration=false

  while [[ $# -gt 0 ]]; do
    case $1 in
      --version)
        version="$2"
        shift 2
        ;;
      --destination)
        install_dir="$2"
        shift 2
        ;;
      --global)
        install_dir="/usr/local/bin"
        shift
        ;;
      --shell-integration)
        shell_integration=true
        shift
        ;;
      --help)
        show_help
        ;;
      *)
        log_error "Unknown option: $1"
        exit 1
        ;;
    esac
  done

  for cmd in curl tar; do
    if ! command -v "$cmd" &> /dev/null; then
      log_error "$cmd is required but not installed"
      exit 1
    fi
  done

  if [ "$version" = "latest" ]; then
    log_info "Fetching latest release version..."
    version=$(get_latest_version)
    if [ -z "$version" ]; then
      log_error "Failed to determine latest version"
      exit 1
    fi
  fi

  if [ ! -d "$install_dir" ]; then
    log_info "Creating directory ${install_dir}"
    mkdir -p "$install_dir" 2>/dev/null || sudo mkdir -p "$install_dir"
  fi

  local tmp_dir
  tmp_dir=$(mktemp -d)
  trap 'rm -rf "$tmp_dir"' EXIT

  local download_url="https://github.com/${REPO}/releases/download/${version}/vignoble-${platform}.tar.gz"
  log_info "Downloading vignoble (${version}) from ${download_url}"

  if ! curl -fsSL "$download_url" | tar -xz -C "$tmp_dir"; then
    log_error "Failed to download vignoble (${version})"
    exit 1
  fi

  log_info "Installing to ${install_dir}/vignoble"
  if [ -w "$install_dir" ]; then
    mv "$tmp_dir/vignoble" "$install_dir/vignoble"
    chmod +x "$install_dir/vignoble"
    "$install_dir/vignoble" postinstall --yes
  else
    sudo mv "$tmp_dir/vignoble" "$install_dir/vignoble"
    sudo chmod +x "$install_dir/vignoble"
    sudo "$install_dir/vignoble" postinstall --yes
  fi

  log_success "vignoble suite installed!"

  if [ "$shell_integration" = true ]; then
    log_info "Installing shell integration..."
    local rc_file
    rc_file=$(detect_shell_rc)
    add_shell_init "$rc_file" || true
  else
    log_info "For shell integration (cep cd + completions), add to your shell configuration file (~/.zshrc, ~/.bashrc):"
    echo "  eval \"\$(vignoble shell-init --with-completion)\""
  fi

  if ! check_path "$install_dir"; then
    log_warning "Installation directory ${install_dir} is not in your PATH"
    echo "Add this to your shell configuration file (~/.zshrc, ~/.bashrc):"
    echo "  export PATH=\"${install_dir}:\$PATH\""
  fi
}

main "$@"
