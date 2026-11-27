# Development & Release Guide

## Workspace Architecture

`vignoble` is a Cargo workspace hosting a single multicall binary (`crates/vignoble`) that dispatches to the suite tools via `argv[0]` (or subcommand fallback):

- **`crates/cli-core`**: Shared CLI foundation (cascading TOML configuration, GitHub release self-update, terminal UI, version helpers).
- **`crates/git-core`**: Shared raw-`git` runner, porcelain worktree parser, batched ref/upstream sync inspector, squash-merge detector, and waterfall stage resolver.
- **`crates/sarment`**: Library crate implementing the `sarment` CLI (`all`, `list`, `clean`).
- **`crates/cep`**: Library crate implementing the `cep` worktree pool CLI (`list`, `create`, `cd`, `detach`, `claim`, `release`, `remove`).
- **`crates/vignoble`**: The single compiled binary (`vignoble`) handling `argv[0]` multicall dispatch, `update`, `postinstall`, `shell-init`, `plugin`, and `agent-state`.

---

## Side-by-Side Release, `-dev`, and `-debug` Binaries

You can keep the production release (`vignoble`, `cep`, `sarment`), a local optimized build (`vignoble-dev`, `cep-dev`, `sarment-dev`), and a local debug build (`vignoble-debug`, `cep-debug`, `sarment-debug`) installed side-by-side in `~/.local/bin`.

`vignoble postinstall` automatically detects how it was built from its executable path (`std::env::current_exe()`), with **no flags required**:

| Executable Path (`current_exe()`) | Detected Target | Target Directory | Links Created |
| :--- | :--- | :--- | :--- |
| `.../target/release/vignoble` | Local release (`-dev`) | `~/.local/bin` | `vignoble-dev -> .../target/release/vignoble`<br>`cep-dev -> vignoble-dev`<br>`sarment-dev -> vignoble-dev` |
| `.../target/debug/vignoble` | Local debug (`-debug`) | `~/.local/bin` | `vignoble-debug -> .../target/debug/vignoble`<br>`cep-debug -> vignoble-debug`<br>`sarment-debug -> vignoble-debug` |
| `~/.local/bin/vignoble` *(or any other directory)* | Installed release (no suffix) | Same directory as `vignoble` | `cep -> vignoble`<br>`sarment -> vignoble` |

### Setting Up Local `-dev` and `-debug` Links

1. **Link the `-debug` suite (`vignoble-debug`, `cep-debug`, `sarment-debug`):**
   ```bash
   cargo run --bin vignoble -- postinstall
   ```
2. **Link the `-dev` suite (`vignoble-dev`, `cep-dev`, `sarment-dev`):**
   ```bash
   cargo build --release
   ./target/release/vignoble postinstall
   ```

Because `~/.local/bin/vignoble-dev` and `~/.local/bin/vignoble-debug` are symlinks pointing directly to `target/release/vignoble` and `target/debug/vignoble`:
- Every subsequent `cargo build` immediately updates `vignoble-debug`, `cep-debug`, and `sarment-debug`.
- Every subsequent `cargo build --release` immediately updates `vignoble-dev`, `cep-dev`, and `sarment-dev`.
- Your production `~/.local/bin/vignoble`, `cep`, and `sarment` remain untouched.

### How `argv[0]` Dispatch & Shell Integration Work With Suffixes

- **`argv[0]` Dispatch:** `invoked_tool_name()` strips `-dev` and `-debug` suffixes from `argv[0]`, so invoking `cep-dev` or `cep-debug` dispatches to `cep`, and `sarment-dev` or `sarment-debug` dispatches to `sarment`.
- **Shell Integration (`cep cd`):** `eval "$(vignoble shell-init --with-completion)"` defines wrapper functions for `cep`, `cep-dev`, and `cep-debug` so `cep-dev cd <target>` and `cep-debug cd <target>` change directories in your active shell.

---

## Running Tests & Quality Checks

```bash
# Format all crates
cargo fmt --all

# Run clippy across all workspace targets
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Run unit and integration tests
cargo test --workspace
```

---

## Creating a Release (`./bin/release`)

Releases are driven by annotated Git tags (`vMAJOR.MINOR.PATCH`) using `./bin/release`:

```bash
./bin/release patch   # 0.1.0 -> 0.1.1
./bin/release minor   # 0.1.0 -> 0.2.0
./bin/release major   # 0.1.0 -> 1.0.0
./bin/release 0.2.0   # Explicit version
```

What `./bin/release` does:
1. Verifies `Cargo.toml`, `Cargo.lock`, and `CHANGELOG.md` have no uncommitted changes.
2. Checks that you are on the `main` branch and that tag `v<VERSION>` does not already exist.
3. Runs `cargo test --workspace` and `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
4. Updates `[workspace.package] version` in `Cargo.toml` and `Cargo.lock`, and updates `CHANGELOG.md`.
5. Creates a signed commit (`Release version <VERSION>`) and annotated tag (`v<VERSION>`).
6. Prompts to push `HEAD` and `v<VERSION>` to `origin`, triggering `.github/workflows/release.yml` to build and publish `vignoble-<platform>.tar.gz` assets.
