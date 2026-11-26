# sarment

Check the sync status, worktree checkout state, and stage progression (`dev`, `staging`, `preprod`, `prod`) of your Git branches.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/themouette/vignoble/main/install.sh | bash
```

This installs `vignoble`, `sarment`, and `cep` to `~/.local/bin`.

To update to the latest release:

```bash
vignoble update
```

## Usage

```bash
# Fetch and show all local branches grouped by stage
sarment

# Skip git fetch
sarment -u

# Include squash-merged PR detection
sarment --squash

# List branch names in a stage (for scripting)
sarment list prod
sarment list dev

# Delete local branches merged in a stage (skips current & worktree-checked-out branches)
sarment clean prod
sarment clean prod -y
```

### Status Symbols

- `*` : upstream only
- `✔` : up to date with upstream
- `⬆` : ahead of upstream (needs push)
- `⬇` : behind upstream (needs pull)
- `⤮` : diverged from upstream
- `?` : local only (no upstream)
- `[wt: <name>]` : checked out in a git worktree

## Configuration

`sarment` shares the unified `vignoble` TOML configuration files, loaded and merged from lowest to highest precedence:

1. `~/.config/vignoble/config.toml`, `~/.config/vignoble.toml`, or `~/.config/vignoble`
2. `~/.vignoble` or `~/.vignoble.toml`
3. Any parent `.vignoble` or `.vignoble.toml` file walking down to the current directory
4. `<repo-root>/.vignoble` or `<repo-root>/.vignoble.toml`
5. `./.vignoble` or `./.vignoble.toml`

### Defaults

Without a config file, `sarment` evaluates stages in this waterfall order and hides any stage whose branch does not exist in the repository:

```toml
[sarment]
remote = "origin"
squash = false
stages = ["main", "master", "prod", "preprod", "staging"]
```

Any branch that does not match a stage is placed in `dev` (`Unmerged`).

### Customizing Stages

Define `stages = [...]` under `[sarment]` (or at the top level) to replace the waterfall order, or define `[sarment.stage.<name>]` (or `[stage.<name>]`) to customize how a specific stage is resolved:

```toml
[sarment]
# Optional: override the full stage waterfall order
stages = ["prod", "preprod", "staging", "partial-staging"]

# 1. Static branch name
[sarment.stage.prod]
branch = "prod"
label = "In production"
color = "green"

# 2. Dynamic target branch (runs once, stdout = branch name)
[sarment.stage.staging]
branch_script = '''
git show origin/preprod:.gitlab-ci.yml | awk -F': ' '/CURRENT_STAGING/ {print $2; exit}'
'''

# 3. Per-branch check script (runs per candidate branch $1; exit 0 = match, first stdout line = annotation)
[sarment.stage.partial-staging]
check_script = '''
branch="${1}"
# return 0 if $branch is partially integrated
'''
```
