use assert_cmd::Command;
use predicates::prelude::*;
use std::{fs, path::Path, process::Command as StdCommand};
use tempfile::tempdir;

#[test]
fn test_multicall_dispatch_via_postinstall_links_and_subcommands() {
    let tmp = tempdir().unwrap();
    let bin_dir = tmp.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();

    let vignoble_src = assert_cmd::cargo::cargo_bin!("vignoble");
    let vignoble_dst = bin_dir.join(format!("vignoble{}", std::env::consts::EXE_SUFFIX));
    fs::copy(vignoble_src, &vignoble_dst).unwrap();

    // Run `vignoble postinstall --yes` on the copied binary in `bin_dir`
    let home_dir = tmp.path().join("home");
    fs::create_dir_all(&home_dir).unwrap();

    Command::new(&vignoble_dst)
        .env("HOME", &home_dir)
        .args(["postinstall", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("sarment"))
        .stdout(predicate::str::contains("cep"))
        .stdout(predicate::str::contains("saisonnier"));

    // Invoke `sarment --help` via the created link in `bin_dir`
    let sarment_link = bin_dir.join(format!("sarment{}", std::env::consts::EXE_SUFFIX));
    Command::new(&sarment_link)
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Check the status of your branches across workflow stages",
        ))
        .stdout(predicate::str::contains("Meaning of the symbols:"));

    // Invoke `cep --help` via the created link in `bin_dir`
    let cep_link = bin_dir.join(format!("cep{}", std::env::consts::EXE_SUFFIX));
    Command::new(&cep_link)
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Manage a pool of long-living, reusable git worktrees",
        ))
        .stdout(predicate::str::contains("Meaning of the symbols:"))
        .stdout(predicate::str::contains(
            "Agent harness icons (set via `vignoble plugin install`):",
        ))
        .stdout(predicate::str::contains("🤖"))
        .stdout(predicate::str::contains("💬"));

    // Verify `vignoble plugin --help` also surfaces the shared agent harness icons section
    Command::new(&vignoble_dst)
        .args(["plugin", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Agent harness icons (set via `vignoble plugin install`):",
        ))
        .stdout(predicate::str::contains("🤖"))
        .stdout(predicate::str::contains("💬"));

    // Invoke `saisonnier --help` via the created link in `bin_dir`
    let saisonnier_link = bin_dir.join(format!("saisonnier{}", std::env::consts::EXE_SUFFIX));
    Command::new(&saisonnier_link)
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Run autonomous coding-agent tasks across a pool of cep worktrees",
        ));

    // Also verify `vignoble sarment --help`, `vignoble cep --help`, and `vignoble saisonnier --help` fallback dispatch
    Command::new(&vignoble_dst)
        .args(["sarment", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Check the status of your branches across workflow stages",
        ));

    Command::new(&vignoble_dst)
        .args(["cep", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Manage a pool of long-living, reusable git worktrees",
        ));

    Command::new(&vignoble_dst)
        .args(["saisonnier", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Run autonomous coding-agent tasks across a pool of cep worktrees",
        ));
}

#[test]
fn test_agent_state_integration_with_cep_list() {
    let tmp = tempdir().unwrap();
    let repo_dir = tmp.path().join("my-repo");
    fs::create_dir_all(&repo_dir).unwrap();

    let init = |args: &[&str], dir: &Path| {
        StdCommand::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
    };
    init(&["init", "-b", "main"], &repo_dir);
    init(&["config", "user.email", "test@example.com"], &repo_dir);
    init(&["config", "user.name", "Test User"], &repo_dir);
    init(&["commit", "--allow-empty", "-m", "initial"], &repo_dir);

    let vignoble_bin = assert_cmd::cargo::cargo_bin!("vignoble");

    // Set agent marker to 🤖 inside the worktree
    Command::new(vignoble_bin)
        .current_dir(&repo_dir)
        .args(["agent-state", "set", "🤖"])
        .assert()
        .success();

    // Verify `vignoble cep list` shows the 🤖 marker
    Command::new(vignoble_bin)
        .current_dir(&repo_dir)
        .args(["cep", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("🤖"));

    // Clear agent marker
    Command::new(vignoble_bin)
        .current_dir(&repo_dir)
        .args(["agent-state", "clear"])
        .assert()
        .success();

    Command::new(vignoble_bin)
        .current_dir(&repo_dir)
        .args(["cep", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("🤖").not());
}
