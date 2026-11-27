# vignoble 🍇

A fast, Git-native workflow suite for managing long-living worktree pools (**`cep`**), branch stage progression (**`sarment`**), and autonomous coding-agent workers (**`saisonnier`**).

- **`vignoble`**: Suite manager (`update`, `postinstall`, `shell-init`, `plugin`) and single multicall binary for the suite
- **`cep`**: Long-living, reusable worktrees named after French grape varieties (*cépages*) with claim/release exclusivity
- **`sarment`**: Branch sync status and stage waterfall (`dev` → `staging` → `preprod` → `prod`) with safe cleanup
- **`saisonnier`**: Self-draining autonomous task queue running configured coding agents across your `cep` pool

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

### `saisonnier` — Autonomous Workers

```bash
# Task & Queue Management
saisonnier                                    # Show queued tasks, active workers, and recent runs
saisonnier enqueue "Add token refresh tests"  # Queue a task and drain onto a free cep
saisonnier edit <task-id>                     # Edit a queued task in $EDITOR (or via -p/-b/--base/--profile)
saisonnier cancel <task-or-run-id>            # Remove a queued task or stop an active worker
saisonnier clean [<run-id>...]                # Remove reviewed run receipts and release held ceps

# Worker & Run Execution
saisonnier logs -f <run-id>                   # View or follow (-f) output logs for an active or past run
saisonnier inspect <task-or-run-id>           # Inspect a queued task or run receipt, prompt, and log tail
saisonnier resume <run-id>                    # Claim a cep and resume the saved agent session interactively
saisonnier tick                               # Claim available ceps for queued tasks and reconcile crashed runs
```

### `vignoble` — Suite & Agent Plugin Management

```bash
vignoble update                   # Self-update vignoble, refresh suite links, and upgrade installed plugins
vignoble postinstall              # Ensure cep, sarment, and saisonnier links exist next to vignoble
vignoble plugin install [agents]  # Install skill & hooks for Pi / Claude Code (interactive menu when omitted)
vignoble plugin uninstall [agents]# Remove skill & hooks (interactive menu when omitted)
vignoble plugin upgrade           # Upgrade already-installed agent plugins to match current version
vignoble plugin status            # Check installed agent plugin status
```

## Configuration & Development

- **Configuration reference:** [`docs/config.md`](docs/config.md)
- **Development, `-dev` / `-debug` binaries & releases:** [`docs/development.md`](docs/development.md)

Configure all tools in `~/.config/vignoble/config.toml` (global) or `./.vignoble.toml` (per-repo/directory):

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

[saisonnier]
default_profile = "pi"
branch_template = "saisonnier/{run_id}"
```
