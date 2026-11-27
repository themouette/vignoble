# `vignoble` Configuration Reference

All tools in the **`vignoble`** suite (**`cep`** and **`sarment`**) share a unified, cascading TOML configuration system.

---

## 1. File Discovery & Cascading Precedence

Configuration files are discovered and merged from **lowest to highest precedence**:

1. **Built-in defaults**
2. **Global XDG / OS config directory:**
   - `~/.config/vignoble/config.toml` (and OS config dir, e.g. `~/Library/Application Support/vignoble/config.toml` on macOS)
   - `~/.config/vignoble.toml`
   - `~/.config/vignoble` (when it is a file)
3. **Home directory:**
   - `~/.vignoble`
   - `~/.vignoble.toml`
4. **Centralized per-repository drop-in files (`<repo>` = main repository directory name, e.g. `web-ui`):**
   - `~/.config/vignoble/repos/<repo>.toml` (and OS config dir `vignoble/repos/<repo>.toml`)
   - `~/.config/vignoble/repos/<repo>` (when it is a file)
5. **Parent directories (walking down from `/` to the parent of the current working directory):**
   - `<parent>/.vignoble`
   - `<parent>/.vignoble.toml`
6. **Main Git repository root (even when running inside a secondary `cep` worktree):**
   - `<main-repo-root>/.vignoble`
   - `<main-repo-root>/.vignoble.toml`
7. **Current working directory:**
   - `./.vignoble`
   - `./.vignoble.toml`
8. **CLI flags** (highest precedence)

> **Note on `[repos."<repo>"]` sections:** Any configuration file (most commonly `~/.config/vignoble/config.toml`) can also include `[repos."<repo>"]` sections. Those inline repo sections are applied immediately after the file's own `[cep]` and `[sarment]` sections.
>
> **Tip:** Run `cep --help` or `sarment --help` in any directory to see the exact list of active configuration files currently in use.

---

## 2. `cep` — Worktree Pool Configuration (`[cep]`)

Configure `cep` under the `[cep]` table (or at the top level of a `.vignoble` / `.vignoble.toml` file):

```toml
[cep]
location = "~/worktrees"
template = "{repo}-worktrees/{name}"
base_ref = "origin/main"
```

### Entries

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `location` | `string` (optional) | `{parent}` (parent directory of the main repository) | Base directory under which relative `template` paths are resolved. Supports `~` home expansion and `{repo}`, `{name}`, `{parent}` placeholders. |
| `template` | `string` | `"{repo}-worktrees/{name}"` | Path template for each worktree. If `template` expands to an absolute path (or starts with `~/...`), it is used directly; otherwise it is joined onto `location`. |
| `base_ref` | `string` (optional) | `HEAD` | Default Git ref used when creating a new detached worktree (`cep create` without `--base`) if `--base` is not passed on the CLI. |

### Template Placeholders

Both `location` and `template` support:
- **`{repo}`**: Directory name of the main Git repository (e.g. `web-ui`).
- **`{name}`**: Short name of the worktree (e.g. `syrah`, `merlot`).
- **`{parent}`**: Absolute path to the parent directory of the main Git repository.
- **`~` / `~/`**: Expanded to the current user's `$HOME` directory.

### Examples

**Default layout (sibling `<repo>-worktrees/<name>` directory):**
```toml
# For a repo at ~/projects/my-app, worktree "syrah" is created at:
# ~/projects/my-app-worktrees/syrah
[cep]
template = "{repo}-worktrees/{name}"
```

**Centralized worktree directory in `~/worktrees/<repo>/<name>`:**
```toml
[cep]
location = "~/worktrees"
template = "{repo}/{name}"
```

---

## 3. `sarment` — Branch Stages & Cleanup Configuration (`[sarment]`)

Configure `sarment` under the `[sarment]` table (or at the top level of a `.vignoble` / `.vignoble.toml` file):

```toml
[sarment]
remote = "origin"
squash = false
stages = ["main", "master", "prod", "preprod", "staging"]
```

### Top-Level `[sarment]` Entries

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `remote` | `string` | `"origin"` | Git remote used for `git fetch`, upstream tracking fallback (`<remote>/<branch>`), and stage branch comparisons. Overridden by `--remote`. |
| `squash` | `bool` | `false` | Enable squash-merge commit detection by default across stages. Overridden by `--squash`. |
| `stages` | `array of strings` | `["main", "master", "prod", "preprod", "staging"]` | Ordered waterfall list of stages from **highest to lowest**. Defining `stages` in a higher-precedence file replaces the list; omitting `stages` inherits the list from lower-precedence files. |

> **Implicit `dev` stage:** Any branch that is not matched by any stage in `stages` automatically falls into `dev` (`Unmerged`).
>
> **`prod` stage alias:** Running `sarment list prod` or `sarment clean prod` in a repository whose production branch is `main` or `master` (and where no `prod` branch exists) automatically aliases `prod` to `main` or `master`.

---

### Per-Stage Configuration (`[sarment.stage.<name>]` or `[stage.<name>]`)

Each stage listed in `stages` can optionally have a `[sarment.stage.<name>]` table to customize how branches are matched and displayed.

If a stage in `stages` has **no** `[sarment.stage.<name>]` table, it defaults to static branch mode (`branch = "<name>"`).

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `branch` | `string` | `"<stage-name>"` | **Mode 1 (Static branch):** Target branch name (checked via `git branch --merged <remote>/<branch>`). If the branch does not exist in the repository, the stage is silently hidden. |
| `branch_script` *(alias `script`)* | `string` | — | **Mode 2 (Dynamic branch script):** Shell script executed **once per run** (`sh -c`). Its first non-empty `stdout` line is used as the target branch name. `$GIT_REMOTE` is passed in the environment. |
| `check_script` | `string` | — | **Mode 3 (Per-branch check script):** Shell script executed **in parallel for each remaining candidate branch** (`$1` and `$BRANCH_NAME` = branch name, `$GIT_REMOTE` = remote). Exit code `0` assigns the branch to this stage; the first non-empty line of `stdout` is displayed as an inline annotation next to the branch. |
| `label` | `string` | Built-in per stage name | Display header for the stage in `sarment` output. |
| `color` | `string` | Built-in per stage name | Header color: `"green"`, `"magenta"` / `"purple"`, `"orange"` / `"yellow"`, `"blue"`, `"cyan"`, `"red"`, `"grey"` / `"gray"`, or `"bold"`. |

#### Built-in Stage Labels & Colors

| Stage Name | Default `label` | Default `color` |
| :--- | :--- | :--- |
| `main`, `master`, `prod` | `"In production"` | `"green"` |
| `preprod` | `"In pre-production"` | `"magenta"` |
| `staging` | `"In staging"` | `"orange"` |
| `partial-staging` | `"In partial staging"` | `"yellow"` |
| `dev` *(implicit fallback)* | `"Unmerged"` | `"blue"` |
| Any custom `<name>` | `"In <name>"` | `"cyan"` |

#### Cascading Stage Merge Rules

- When multiple config files configure the same `[sarment.stage.<name>]`, `label` and `color` are inherited unless overridden.
- Resolution modes (`branch`, `branch_script`, `check_script`) are mutually exclusive: if a higher-precedence config sets `check_script`, it replaces a lower-precedence `branch` or `branch_script` on that stage.

### Stage Examples

**1. Dynamic `staging` branch resolved from `.gitlab-ci.yml` (`branch_script`):**
```toml
[sarment.stage.staging]
label = "In staging"
color = "orange"
branch_script = '''
for ref in origin/preprod origin/prod origin/master origin/main; do
  if git show "$ref:.gitlab-ci.yml" >/dev/null 2>&1; then
    git show "$ref:.gitlab-ci.yml" | awk -F': ' '/CURRENT_STAGING/ {print $2; exit}'
    exit 0
  fi
done
'''
```

**2. Full vs. partial multi-service staging (`check_script` with inline annotations):**
```toml
[sarment]
stages = ["main", "prod", "preprod", "staging", "partial-staging"]

[sarment.stage.staging]
label = "In staging"
color = "orange"
check_script = '''
branch="$1"
# Exit 0 when $branch is integrated into all target staging branches
'''

[sarment.stage.partial-staging]
label = "In partial staging"
color = "yellow"
check_script = '''
branch="$1"
# Exit 0 when $branch is integrated into at least one target staging branch.
# First line of stdout is shown next to the branch in `sarment`:
echo "1/2 staging branches"
exit 0
'''
```

---

## 4. Centralized Per-Repository Configuration (No Files in Repo Root)

When working in shared repositories where you cannot commit a `.vignoble.toml` file (and want all your configuration centralized in `~/.config/vignoble/`), `vignoble` resolves the **main repository directory name** (`<repo>`, e.g. `web-ui` or `dd-source`) via `git rev-parse --git-common-dir`—so it matches `"web-ui"` even when your shell is inside a linked `cep` worktree like `~/worktrees/web-ui/syrah`.

You can organize per-repository configuration in two ways (or combine them):

### Option A: Inline `[repos."<repo>"]` sections in `~/.config/vignoble/config.toml`

Best for keeping everything in a single dotfiles-tracked file with concise per-repository overrides:

```toml
# Global defaults for all repositories
[cep]
location = "~/worktrees"
template = "{repo}/{name}"

[sarment]
remote = "origin"
stages = ["main", "master", "prod", "preprod", "staging"]

# Per-repository override for `web-ui`
[repos."web-ui".sarment]
stages = ["prod", "preprod", "staging"]

[repos."web-ui".sarment.stage.staging]
branch_script = '''
git show origin/preprod:.gitlab-ci.yml | awk -F': ' '/CURRENT_STAGING/ {print $2; exit}'
'''

# Per-repository override for `dd-source` (both sarment and cep)
[repos."dd-source".cep]
location = "/Volumes/fast-ssd/worktrees"

[repos."dd-source".sarment]
stages = ["main", "staging", "partial-staging"]
```

### Option B: Drop-in files in `~/.config/vignoble/repos/<repo>.toml`

Best when a repository has longer scripts (`branch_script` / `check_script`) that you want to isolate in a dedicated file:

```text
~/.config/vignoble/
├── config.toml              # Global defaults
└── repos/
    ├── web-ui.toml          # Automatically loaded when main repo is "web-ui"
    └── dd-source.toml       # Automatically loaded when main repo is "dd-source"
```

Inside `~/.config/vignoble/repos/web-ui.toml`, use the standard top-level `[sarment]` / `[cep]` schema (no `[repos."web-ui"]` prefix required):

```toml
[sarment]
stages = ["prod", "preprod", "staging"]

[sarment.stage.staging]
branch_script = '''
git show origin/preprod:.gitlab-ci.yml | awk -F': ' '/CURRENT_STAGING/ {print $2; exit}'
'''
```
