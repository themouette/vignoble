# cep

Manage a pool of long-living, reusable Git worktrees named after French grape varieties (*cépages*).

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/themouette/vignoble/main/install.sh | bash
```

This installs `vignoble`, `sarment`, and `cep` to `~/.local/bin`.

For directory navigation (`cep cd`), add shell integration to your `~/.zshrc` or `~/.bashrc`:

```bash
eval "$(vignoble shell-init)"
```

## Usage

```bash
# List all worktrees (main, detached pool worktrees, and claimed worktrees)
cep

# Create a new detached worktree in the pool with a random French grape variety name
cep create

# Create a named worktree and check out a branch
cep create syrah -b feat/login

# Claim an available worktree from the pool (or --create one if none is free)
cep claim -b feat/payment -m "working on payment"
cep claim --create --print-path

# Detach HEAD in a worktree while preserving its claim lock
cep detach syrah

# Release a worktree back to the pool (detaches HEAD and unlocks)
cep release syrah

# Navigate between worktrees (by name, branch, path, or '-' for previous)
cep cd syrah
cep cd feat/payment
cep cd -
cep cd

# Remove a worktree from the pool
cep remove syrah
```

## Configuration

`cep` shares the unified `vignoble` TOML configuration files, loaded and merged from lowest to highest precedence:

1. `~/.config/vignoble/config.toml`, `~/.config/vignoble.toml`, or `~/.config/vignoble`
2. `~/.vignoble` or `~/.vignoble.toml`
3. Any parent `.vignoble` or `.vignoble.toml` file walking down to the current directory
4. `<repo-root>/.vignoble` or `<repo-root>/.vignoble.toml`
5. `./.vignoble` or `./.vignoble.toml`

```toml
[cep]
# Optional base directory for new worktrees (defaults to parent of main repository).
# Supports `~` expansion.
location = "~/worktrees"

# Relative path template under `location` (or absolute path template).
# Placeholders: {repo}, {name}, {parent}
template = "{repo}-worktrees/{name}"

# Optional default base ref when creating a detached worktree
# base_ref = "origin/main"
```
