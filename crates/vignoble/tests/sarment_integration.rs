use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use tempfile::{tempdir, TempDir};

struct TestGitEnv {
    _tmp: TempDir,
    #[allow(dead_code)]
    remote_dir: PathBuf,
    local_dir: PathBuf,
    worktrees_dir: PathBuf,
    sarment_bin: PathBuf,
}

fn run_git(cwd: &Path, args: &[&str]) {
    let output = StdCommand::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Test User")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test User")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .expect("failed to execute git");
    assert!(
        output.status.success(),
        "git {:?} failed in {:?}: {}",
        args,
        cwd,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn commit_file(cwd: &Path, filename: &str, content: &str, message: &str) {
    fs::write(cwd.join(filename), content).unwrap();
    run_git(cwd, &["add", filename]);
    run_git(cwd, &["commit", "-m", message]);
}

impl TestGitEnv {
    fn setup() -> Self {
        let tmp = tempdir().unwrap();
        let remote_dir = tmp.path().join("remote.git");
        let local_dir = tmp.path().join("local");
        let worktrees_dir = tmp.path().join("worktrees");
        let bin_dir = tmp.path().join("bin");

        fs::create_dir_all(&remote_dir).unwrap();
        fs::create_dir_all(&local_dir).unwrap();
        fs::create_dir_all(&worktrees_dir).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();

        let vignoble_src = assert_cmd::cargo::cargo_bin!("vignoble");
        let sarment_bin = bin_dir.join(format!("sarment{}", std::env::consts::EXE_SUFFIX));
        #[cfg(unix)]
        std::os::unix::fs::symlink(vignoble_src, &sarment_bin).unwrap();
        #[cfg(windows)]
        fs::copy(vignoble_src, &sarment_bin).unwrap();

        run_git(&remote_dir, &["init", "--bare", "--initial-branch=main"]);
        run_git(&local_dir, &["init", "--initial-branch=main"]);
        run_git(
            &local_dir,
            &["remote", "add", "origin", remote_dir.to_str().unwrap()],
        );

        commit_file(&local_dir, "README.md", "initial\n", "Initial commit");
        run_git(&local_dir, &["push", "-u", "origin", "main"]);

        Self {
            _tmp: tmp,
            remote_dir,
            local_dir,
            worktrees_dir,
            sarment_bin,
        }
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::new(&self.sarment_bin);
        cmd.current_dir(&self.local_dir);
        cmd.env("HOME", self._tmp.path());
        cmd
    }
}

#[test]
fn test_all_and_list_with_default_stages_and_sync_status() {
    let env = TestGitEnv::setup();

    run_git(&env.local_dir, &["checkout", "-b", "feat/merged-in-main"]);
    commit_file(
        &env.local_dir,
        "merged.txt",
        "hello\n",
        "Add merged feature",
    );
    run_git(
        &env.local_dir,
        &["push", "-u", "origin", "feat/merged-in-main"],
    );
    run_git(&env.local_dir, &["checkout", "main"]);
    run_git(
        &env.local_dir,
        &[
            "merge",
            "--no-ff",
            "feat/merged-in-main",
            "-m",
            "Merge feat",
        ],
    );
    run_git(&env.local_dir, &["push", "origin", "main"]);

    run_git(&env.local_dir, &["checkout", "-b", "feat/need-push"]);
    commit_file(&env.local_dir, "push.txt", "v1\n", "Push v1");
    run_git(&env.local_dir, &["push", "-u", "origin", "feat/need-push"]);
    commit_file(&env.local_dir, "push.txt", "v2\n", "Push v2");

    run_git(&env.local_dir, &["checkout", "-b", "feat/local-only"]);
    commit_file(&env.local_dir, "local.txt", "local\n", "Local commit");

    run_git(&env.local_dir, &["checkout", "main"]);

    env.cmd()
        .arg("-u")
        .assert()
        .success()
        .stdout(predicate::str::contains("Unmerged"))
        .stdout(predicate::str::contains("feat/need-push"))
        .stdout(predicate::str::contains("(new local commits)"))
        .stdout(predicate::str::contains("feat/local-only"))
        .stdout(predicate::str::contains("(no upstream)"))
        .stdout(predicate::str::contains("In production"))
        .stdout(predicate::str::contains("(main)"))
        .stdout(predicate::str::contains("feat/merged-in-main"))
        .stdout(predicate::str::contains(
            "You have 2 branches in active development",
        ));

    env.cmd()
        .args(["-u", "list", "prod"])
        .assert()
        .success()
        .stdout("feat/merged-in-main\n");

    env.cmd()
        .args(["-u", "list", "dev"])
        .assert()
        .success()
        .stdout(predicate::str::contains("feat/need-push"))
        .stdout(predicate::str::contains("feat/local-only"));
}

#[test]
fn test_worktree_indicator_and_clean_protection() {
    let env = TestGitEnv::setup();

    run_git(&env.local_dir, &["checkout", "-b", "feat/deletable"]);
    commit_file(&env.local_dir, "a.txt", "a\n", "Add a");
    run_git(&env.local_dir, &["checkout", "main"]);
    run_git(
        &env.local_dir,
        &["merge", "--no-ff", "feat/deletable", "-m", "Merge a"],
    );

    run_git(&env.local_dir, &["checkout", "-b", "feat/in-worktree"]);
    commit_file(&env.local_dir, "b.txt", "b\n", "Add b");
    run_git(&env.local_dir, &["checkout", "main"]);
    run_git(
        &env.local_dir,
        &["merge", "--no-ff", "feat/in-worktree", "-m", "Merge b"],
    );
    run_git(&env.local_dir, &["push", "origin", "main"]);

    let wt_path = env.worktrees_dir.join("pool-alpha");
    run_git(
        &env.local_dir,
        &[
            "worktree",
            "add",
            wt_path.to_str().unwrap(),
            "feat/in-worktree",
        ],
    );

    let wt_detached = env.worktrees_dir.join("pool-unclaimed");
    run_git(
        &env.local_dir,
        &[
            "worktree",
            "add",
            "--detach",
            wt_detached.to_str().unwrap(),
            "main",
        ],
    );

    env.cmd()
        .arg("-u")
        .assert()
        .success()
        .stdout(predicate::str::contains("[wt: pool-alpha]"));

    env.cmd()
        .args(["-u", "-y", "clean", "prod"])
        .assert()
        .success()
        .stdout(predicate::str::contains("checked out in worktree"))
        .stdout(predicate::str::contains("feat/deletable"))
        .stdout(predicate::str::contains("has been deleted"));

    env.cmd()
        .args(["-u", "list", "prod"])
        .assert()
        .success()
        .stdout("feat/in-worktree\n");
}

#[test]
fn test_custom_stages_with_branch_script_and_check_script() {
    let env = TestGitEnv::setup();

    run_git(&env.local_dir, &["checkout", "-b", "preprod"]);
    commit_file(
        &env.local_dir,
        ".gitlab-ci.yml",
        "CURRENT_STAGING: staging-42\n",
        "Add gitlab ci",
    );
    run_git(&env.local_dir, &["push", "-u", "origin", "preprod"]);

    run_git(&env.local_dir, &["checkout", "-b", "staging-42"]);
    run_git(&env.local_dir, &["push", "-u", "origin", "staging-42"]);

    run_git(&env.local_dir, &["checkout", "-b", "feat/in-staging"]);
    commit_file(&env.local_dir, "stg.txt", "stg\n", "Staging feature");
    run_git(&env.local_dir, &["checkout", "staging-42"]);
    run_git(
        &env.local_dir,
        &[
            "merge",
            "--no-ff",
            "feat/in-staging",
            "-m",
            "Merge to staging",
        ],
    );
    run_git(&env.local_dir, &["push", "origin", "staging-42"]);

    run_git(&env.local_dir, &["checkout", "preprod"]);
    run_git(&env.local_dir, &["checkout", "-b", "feat/partial-only"]);
    commit_file(&env.local_dir, "part.txt", "part\n", "Partial feature");

    run_git(&env.local_dir, &["checkout", "preprod"]);

    let cfg = r#"
[sarment]
stages = ["main", "preprod", "staging", "partial-staging"]

[sarment.stage.staging]
branch_script = '''
git show origin/preprod:.gitlab-ci.yml | awk -F': ' '/CURRENT_STAGING/ {print $2}'
'''

[sarment.stage.partial-staging]
check_script = '''
if [ "$1" = "feat/partial-only" ]; then
  echo "1/2 services"
  exit 0
fi
exit 1
'''
"#;
    fs::write(env.local_dir.join(".vignoble.toml"), cfg).unwrap();

    env.cmd()
        .args(["-u", "list", "staging"])
        .assert()
        .success()
        .stdout("feat/in-staging\n");

    env.cmd()
        .args(["-u", "list", "partial-staging"])
        .assert()
        .success()
        .stdout("feat/partial-only\n");

    env.cmd()
        .arg("-u")
        .assert()
        .success()
        .stdout(predicate::str::contains("(staging-42)"))
        .stdout(predicate::str::contains("1/2 services"));
}

#[test]
fn test_unified_vignoble_config_precedence() {
    let env = TestGitEnv::setup();

    run_git(&env.local_dir, &["checkout", "-b", "feat/cfg-check"]);
    commit_file(&env.local_dir, "cfg.txt", "cfg\n", "Config feature");
    run_git(&env.local_dir, &["checkout", "main"]);

    let extless_cfg = r#"
stages = ["main", "partial-staging"]

[stage.partial-staging]
check_script = '''
if [ "$1" = "feat/cfg-check" ]; then
  echo "from-dot-vignoble"
  exit 0
fi
exit 1
'''
"#;
    fs::write(env.local_dir.join(".vignoble"), extless_cfg).unwrap();

    env.cmd()
        .args(["-u", "list", "partial-staging"])
        .assert()
        .success()
        .stdout("feat/cfg-check\n");

    let toml_cfg = r#"
[sarment.stage.partial-staging]
check_script = '''
if [ "$1" = "feat/cfg-check" ]; then
  echo "from-vignoble-toml"
  exit 0
fi
exit 1
'''
"#;
    fs::write(env.local_dir.join(".vignoble.toml"), toml_cfg).unwrap();

    env.cmd()
        .arg("-u")
        .assert()
        .success()
        .stdout(predicate::str::contains("from-vignoble-toml"));
}

#[test]
fn test_squash_merge_detection_on_demand() {
    let env = TestGitEnv::setup();

    run_git(&env.local_dir, &["checkout", "-b", "feat/squashed-pr"]);
    commit_file(
        &env.local_dir,
        "feature.rs",
        "pub fn compute() -> i32 {\n    42\n}\n",
        "Implement compute helper for feature",
    );

    run_git(&env.local_dir, &["checkout", "main"]);
    run_git(&env.local_dir, &["merge", "--squash", "feat/squashed-pr"]);
    run_git(
        &env.local_dir,
        &[
            "commit",
            "-m",
            "Implement compute helper for feature (#123)",
        ],
    );
    run_git(&env.local_dir, &["push", "origin", "main"]);

    env.cmd()
        .args(["-u", "list", "dev"])
        .assert()
        .success()
        .stdout("feat/squashed-pr\n");

    env.cmd()
        .args(["-u", "--squash", "list", "prod"])
        .assert()
        .success()
        .stdout("feat/squashed-pr\n");
}

#[test]
fn test_centralized_per_repo_config_inline_and_drop_in() {
    let env = TestGitEnv::setup();

    run_git(&env.local_dir, &["checkout", "-b", "feat/centralized"]);
    commit_file(&env.local_dir, "c.txt", "c\n", "Centralized feature");
    run_git(&env.local_dir, &["checkout", "main"]);

    // 1. Inline `[repos."local".sarment]` in `~/.config/vignoble/config.toml`
    let global_cfg_dir = env._tmp.path().join(".config").join("vignoble");
    fs::create_dir_all(&global_cfg_dir).unwrap();
    let global_cfg = r#"
[repos."local".sarment]
stages = ["main", "partial-staging"]

[repos."local".sarment.stage.partial-staging]
check_script = '''
if [ "$1" = "feat/centralized" ]; then
  echo "from-inline-repos"
  exit 0
fi
exit 1
'''
"#;
    fs::write(global_cfg_dir.join("config.toml"), global_cfg).unwrap();

    env.cmd()
        .arg("-u")
        .assert()
        .success()
        .stdout(predicate::str::contains("from-inline-repos"));

    // 2. Drop-in `~/.config/vignoble/repos/local.toml` overrides inline `[repos."local"]`
    let repos_dir = global_cfg_dir.join("repos");
    fs::create_dir_all(&repos_dir).unwrap();
    let drop_in_cfg = r#"
[sarment.stage.partial-staging]
check_script = '''
if [ "$1" = "feat/centralized" ]; then
  echo "from-repos-drop-in"
  exit 0
fi
exit 1
'''
"#;
    fs::write(repos_dir.join("local.toml"), drop_in_cfg).unwrap();

    env.cmd()
        .arg("-u")
        .assert()
        .success()
        .stdout(predicate::str::contains("from-repos-drop-in"));
}
