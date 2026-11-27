# vignoble 🍇

A fast, Git-native workflow suite for managing long-living worktree pools (**`cep`**) and branch stage progression (**`sarment`**).

- **`vignoble`**: Suite manager (`update`, `postinstall`, `shell-init`, `plugin`) and single multicall binary for the suite
- **`cep`**: Long-living, reusable worktrees named after French grape varieties (*cépages*) with claim/release exclusivity
- **`sarment`**: Branch sync status and stage waterfall (`dev` → `staging` → `preprod` → `prod`) with safe cleanup

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/themouette/vignoble/main/install.sh | bash
```

Enable `cep cd` navigation and tab completions in your shell (`~/.zshrc` or `~/.bashrc`):

```bash
eval "$(vignoble shell-init --with-completion)"
```

## Quick Start

### `cep` — Long-Living Worktrees

```bash
cep                           # List all worktrees (including detached/unclaimed)
cep create                    # Create a detached worktree with a random grape name (e.g. 'syrah')
cep claim -b feat/auth        # Claim a free worktree and check out 'feat/auth'
cep cd syrah                  # Jump by worktree name, branch name, or path ('cep cd -' for previous)
cep detach                    # Detach HEAD to free the branch while keeping your claim
cep release                   # Detach HEAD and unlock the worktree back to the pool
cep rm syrah                  # Remove a worktree
```

### `sarment` — Branch Stages & Cleanup

```bash
sarment                       # Fetch and show local branches grouped by stage (dev/staging/preprod/prod)
sarment -u                    # Skip fetch
sarment --squash              # Include squash-merged PR detection
sarment ls prod               # List branch names in a stage
sarment clean prod            # Delete branches merged in prod (skips branches checked out in any cep)
```

### `vignoble` — Suite & Agent Plugin Management

```bash
vignoble update                   # Self-update vignoble, refresh suite links, and upgrade installed plugins
vignoble postinstall              # Ensure cep and sarment links exist next to vignoble
vignoble plugin install [agents]  # Install skill & hooks for Pi / Claude Code (interactive menu when omitted)
vignoble plugin uninstall [agents]# Remove skill & hooks (interactive menu when omitted)
vignoble plugin upgrade           # Upgrade already-installed agent plugins to match current version
vignoble plugin status            # Check installed agent plugin status
```

## Configuration

Configure both tools in `~/.config/vignoble.toml` (global) or `./.vignoble.toml` (per-repo/directory):

```toml
[cep]
location = "~/worktrees"
template = "{repo}/{name}" # placeholders: {repo}, {name}, {parent}

[sarment]
remote = "origin"
squash = false
stages = ["main", "master", "prod", "preprod", "staging"]

[sarment.stage.staging]
branch_script = '''
git show origin/preprod:.gitlab-ci.yml | awk -F': ' '/CURRENT_STAGING/ {print $2; exit}'
'''
```
