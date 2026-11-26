#!/usr/bin/env bash
set -e

# Installation script for the vignoble suite (vignoble, sarment, and future cep)
# Usage: curl -fsSL https://raw.githubusercontent.com/themouette/vignoble/main/install.sh | bash

REPO="themouette/vignoble"
DEFAULT_TOOLS=("vignoble" "sarment")

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
    curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | bash -s -- [OPTIONS] [TOOLS...]

OPTIONS:
    --version <VERSION>        Version to install (default: latest)
                               Examples: v0.1.0, latest

    --destination <PATH>       Custom installation directory
                               Overrides --global

    --global                   Install to /usr/local/bin (requires sudo)
                               Default is ~/.local/bin (no sudo required)

    --help                     Show this help message

TOOLS:
    Default installed tools: ${DEFAULT_TOOLS[*]}
    For shell integration (e.g. cep worktree navigation), add to your shell config:
      eval "\$(vignoble shell-init)"

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

install_tool() {
  local tool="$1"
  local version="$2"
  local platform="$3"
  local install_dir="$4"
  local tmp_dir="$5"

  local download_url="https://github.com/${REPO}/releases/download/${version}/${tool}-${platform}.tar.gz"
  log_info "Downloading ${tool} (${version}) from ${download_url}"

  if ! curl -fsSL "$download_url" | tar -xz -C "$tmp_dir"; then
    log_error "Failed to download ${tool} (${version})"
    exit 1
  fi

  log_info "Installing to ${install_dir}/${tool}"
  if [ -w "$install_dir" ]; then
    mv "$tmp_dir/$tool" "$install_dir/$tool"
    chmod +x "$install_dir/$tool"
  else
    sudo mv "$tmp_dir/$tool" "$install_dir/$tool"
    sudo chmod +x "$install_dir/$tool"
  fi

  log_success "${tool} installed!"
}

main() {
  local platform
  platform=$(detect_platform)
  local version="latest"
  local install_dir="$HOME/.local/bin"
  local tools=()

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
      --help)
        show_help
        ;;
      -*)
        log_error "Unknown option: $1"
        exit 1
        ;;
      *)
        tools+=("$1")
        shift
        ;;
    esac
  done

  if [ ${#tools[@]} -eq 0 ]; then
    tools=("${DEFAULT_TOOLS[@]}")
  fi

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

  for tool in "${tools[@]}"; do
    install_tool "$tool" "$version" "$platform" "$install_dir" "$tmp_dir"
  done

  log_info "For shell integration, add to your shell configuration file (~/.zshrc, ~/.bashrc):"
  echo "  eval \"\$(vignoble shell-init)\""

  if ! check_path "$install_dir"; then
    log_warning "Installation directory ${install_dir} is not in your PATH"
    echo "Add this to your shell configuration file (~/.zshrc, ~/.bashrc):"
    echo "  export PATH=\"${install_dir}:\$PATH\""
  fi
}

main "$@"
