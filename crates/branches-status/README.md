# branches-status (`git bs`)

Check the sync status, worktree checkout state, and stage progression (`dev`, `staging`, `preprod`, `prod`) of your Git branches.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/themouette/git-workflow/main/install.sh | bash
```

This installs `branches-status` to `~/.local/bin` and links `git-bs`, so you can run either `branches-status` or `git bs`.

To update to the latest release:

```bash
branches-status update
```

## Usage

```bash
# Fetch and show all local branches grouped by stage
branches-status

# Skip git fetch
branches-status -u

# Include squash-merged PR detection
branches-status --squash

# List branch names in a stage (for scripting)
branches-status list prod
branches-status list dev

# Delete local branches merged in a stage (skips current & worktree-checked-out branches)
branches-status clean prod
branches-status clean prod -y
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

`branches-status` loads and merges TOML configuration files from lowest to highest precedence:

1. `~/.config/branches-status/config.toml` or `~/.config/branches-status`
2. `~/.branches-status`
3. Any parent `.branches-status` directory walking down to the current directory
4. `<repo-root>/.branches-status` and `./.branches-status`

### Defaults

Without a config file, `branches-status` evaluates stages in this waterfall order and hides any stage whose branch does not exist in the repository:

```toml
remote = "origin"
squash = false
stages = ["main", "master", "prod", "preprod", "staging"]
```

Any branch that does not match a stage is placed in `dev` (`Unmerged`).

### Customizing Stages

Define `stages = [...]` to replace the waterfall order, or define `[stage.<name>]` to customize how a specific stage is resolved:

```toml
# Optional: override the full stage waterfall order
stages = ["prod", "preprod", "staging", "partial-staging"]

# 1. Static branch name
[stage.prod]
branch = "prod"
label = "In production"
color = "green"

# 2. Dynamic target branch (runs once, stdout = branch name)
[stage.staging]
branch_script = '''
git show origin/preprod:.gitlab-ci.yml | awk -F': ' '/CURRENT_STAGING/ {print $2; exit}'
'''

# 3. Per-branch check script (runs per candidate branch $1; exit 0 = match, first stdout line = annotation)
[stage.partial-staging]
check_script = '''
branch="${1}"
# return 0 if $branch is partially integrated
'''
```
