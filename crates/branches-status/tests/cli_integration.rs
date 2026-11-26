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

        fs::create_dir_all(&remote_dir).unwrap();
        fs::create_dir_all(&local_dir).unwrap();
        fs::create_dir_all(&worktrees_dir).unwrap();

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
        }
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("branches-status").unwrap();
        cmd.current_dir(&self.local_dir);
        // Isolate HOME so user's global ~/.config/branches-status doesn't affect tests
        cmd.env("HOME", self._tmp.path());
        cmd
    }
}

#[test]
fn test_all_and_list_with_default_stages_and_sync_status() {
    let env = TestGitEnv::setup();

    // Create a branch merged into main and pushed to origin (UpToDate in prod)
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

    // Create a branch in dev that needs push
    run_git(&env.local_dir, &["checkout", "-b", "feat/need-push"]);
    commit_file(&env.local_dir, "push.txt", "v1\n", "Push v1");
    run_git(&env.local_dir, &["push", "-u", "origin", "feat/need-push"]);
    commit_file(&env.local_dir, "push.txt", "v2\n", "Push v2");

    // Create a local-only branch in dev (no upstream)
    run_git(&env.local_dir, &["checkout", "-b", "feat/local-only"]);
    commit_file(&env.local_dir, "local.txt", "local\n", "Local commit");

    run_git(&env.local_dir, &["checkout", "main"]);

    // Verify `all` output shows Unmerged and In production (main)
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

    // Verify `list prod` aliases to `main` when `prod` branch does not exist
    env.cmd()
        .args(["-u", "list", "prod"])
        .assert()
        .success()
        .stdout("feat/merged-in-main\n");

    // Verify `list dev` lists the unmerged branches
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

    // Create two branches merged into main
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

    // Check out `feat/in-worktree` in a secondary worktree (`pool-alpha`)
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

    // Also create a detached HEAD worktree (`pool-unclaimed`) to ensure detached worktrees parse cleanly
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

    // 1. `all` shows `[wt: pool-alpha]` indicator for `feat/in-worktree`
    env.cmd()
        .arg("-u")
        .assert()
        .success()
        .stdout(predicate::str::contains("[wt: pool-alpha]"));

    // 2. `clean prod -y` skips `feat/in-worktree` and deletes `feat/deletable`
    env.cmd()
        .args(["-u", "-y", "clean", "prod"])
        .assert()
        .success()
        .stdout(predicate::str::contains("checked out in worktree"))
        .stdout(predicate::str::contains("feat/deletable"))
        .stdout(predicate::str::contains("has been deleted"));

    // Confirm `feat/in-worktree` still exists while `feat/deletable` is gone
    env.cmd()
        .args(["-u", "list", "prod"])
        .assert()
        .success()
        .stdout("feat/in-worktree\n");
}

#[test]
fn test_custom_stages_with_branch_script_and_check_script() {
    let env = TestGitEnv::setup();

    // Create `preprod` and dynamic `staging-42` on remote
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

    // Create a branch merged into `staging-42`
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

    // Create a branch that matches `partial-staging` via `check_script`
    run_git(&env.local_dir, &["checkout", "preprod"]);
    run_git(&env.local_dir, &["checkout", "-b", "feat/partial-only"]);
    commit_file(&env.local_dir, "part.txt", "part\n", "Partial feature");

    run_git(&env.local_dir, &["checkout", "preprod"]);

    // Write local `.branches-status` config
    let cfg = r#"
stages = ["main", "preprod", "staging", "partial-staging"]

[stage.staging]
branch_script = '''
git show origin/preprod:.gitlab-ci.yml | awk -F': ' '/CURRENT_STAGING/ {print $2}'
'''

[stage.partial-staging]
check_script = '''
if [ "$1" = "feat/partial-only" ]; then
  echo "1/2 services"
  exit 0
fi
exit 1
'''
"#;
    fs::write(env.local_dir.join(".branches-status"), cfg).unwrap();

    // Verify `list staging` finds `feat/in-staging` via dynamic `branch_script`
    env.cmd()
        .args(["-u", "list", "staging"])
        .assert()
        .success()
        .stdout("feat/in-staging\n");

    // Verify `list partial-staging` finds `feat/partial-only` via `check_script`
    env.cmd()
        .args(["-u", "list", "partial-staging"])
        .assert()
        .success()
        .stdout("feat/partial-only\n");

    // Verify `all` prints the annotation `1/2 services` and `(staging-42)` reference
    env.cmd()
        .arg("-u")
        .assert()
        .success()
        .stdout(predicate::str::contains("(staging-42)"))
        .stdout(predicate::str::contains("1/2 services"));
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

    // Squash merge `feat/squashed-pr` onto `main`
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

    // Without `--squash`, `feat/squashed-pr` is in `dev`
    env.cmd()
        .args(["-u", "list", "dev"])
        .assert()
        .success()
        .stdout("feat/squashed-pr\n");

    // With `--squash`, `feat/squashed-pr` is detected in `prod` (`main`)
    env.cmd()
        .args(["-u", "--squash", "list", "prod"])
        .assert()
        .success()
        .stdout("feat/squashed-pr\n");
}
