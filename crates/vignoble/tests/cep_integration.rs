use assert_cmd::Command;
use cep::names::CEPAGES;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use tempfile::{tempdir, TempDir};

struct TestGitEnv {
    _tmp: TempDir,
    local_dir: PathBuf,
    cep_bin: PathBuf,
}

fn run_git(cwd: &Path, args: &[&str]) -> String {
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
    String::from_utf8_lossy(&output.stdout).trim().to_string()
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
        let local_dir = tmp.path().join("my-repo");
        let bin_dir = tmp.path().join("bin");

        fs::create_dir_all(&remote_dir).unwrap();
        fs::create_dir_all(&local_dir).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();

        let vignoble_src = assert_cmd::cargo::cargo_bin!("vignoble");
        let cep_bin = bin_dir.join(format!("cep{}", std::env::consts::EXE_SUFFIX));
        #[cfg(unix)]
        std::os::unix::fs::symlink(vignoble_src, &cep_bin).unwrap();
        #[cfg(windows)]
        fs::copy(vignoble_src, &cep_bin).unwrap();

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
            local_dir,
            cep_bin,
        }
    }

    fn cmd(&self) -> Command {
        self.cmd_in(&self.local_dir)
    }

    fn cmd_in(&self, dir: &Path) -> Command {
        let mut cmd = Command::new(&self.cep_bin);
        cmd.current_dir(dir);
        cmd.env("HOME", self._tmp.path());
        cmd
    }
}

#[test]
fn test_create_default_cepage_detached_and_custom_vignoble_config() {
    let env = TestGitEnv::setup();

    let output = env.cmd().arg("create").output().unwrap();
    assert!(output.status.success());

    let default_wt_parent = env._tmp.path().join("my-repo-worktrees");
    assert!(default_wt_parent.is_dir());

    let entries: Vec<PathBuf> = fs::read_dir(&default_wt_parent)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(entries.len(), 1);

    let wt_dir = &entries[0];
    let wt_name = wt_dir.file_name().unwrap().to_str().unwrap();
    assert!(
        CEPAGES.contains(&wt_name),
        "expected '{}' to be a valid cepage",
        wt_name
    );

    let symbolic = StdCommand::new("git")
        .args(["-C", wt_dir.to_str().unwrap(), "symbolic-ref", "-q", "HEAD"])
        .output()
        .unwrap();
    assert!(!symbolic.status.success(), "expected detached HEAD");

    let branch_check = StdCommand::new("git")
        .args([
            "-C",
            env.local_dir.to_str().unwrap(),
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{}", wt_name),
        ])
        .output()
        .unwrap();
    assert!(
        !branch_check.status.success(),
        "expected no branch created for detached cepage worktree"
    );

    let custom_cfg = r#"
[cep]
location = "~/custom-pool"
template = "{repo}/{name}"
"#;
    fs::write(env.local_dir.join(".vignoble.toml"), custom_cfg).unwrap();

    env.cmd()
        .args([
            "create",
            "syrah",
            "-b",
            "feat/custom-branch",
            "-m",
            "agent-1",
        ])
        .assert()
        .success();

    let expected_custom_path = env
        ._tmp
        .path()
        .join("custom-pool")
        .join("my-repo")
        .join("syrah");
    assert!(expected_custom_path.is_dir());

    let branch_in_syrah = run_git(
        &expected_custom_path,
        &["rev-parse", "--abbrev-ref", "HEAD"],
    );
    assert_eq!(branch_in_syrah, "feat/custom-branch");
}

#[test]
fn test_list_displays_main_detached_claimed_branches_and_agent_marker() {
    let env = TestGitEnv::setup();

    env.cmd().args(["create", "merlot"]).assert().success();
    env.cmd()
        .args([
            "create",
            "syrah",
            "-b",
            "feat/syrah-work",
            "-m",
            "fixing-auth",
        ])
        .assert()
        .success();

    run_git(&env.local_dir, &["config", "cep.state.syrah.marker", "🤖"]);

    env.cmd()
        .assert()
        .success()
        .stdout(predicate::str::contains("my-repo"))
        .stdout(predicate::str::contains("(main)"))
        .stdout(predicate::str::contains("merlot"))
        .stdout(predicate::str::contains("(detached @"))
        .stdout(predicate::str::contains("available"))
        .stdout(predicate::str::contains("syrah"))
        .stdout(predicate::str::contains("feat/syrah-work"))
        .stdout(predicate::str::contains("🔒 claimed (fixing-auth)"))
        .stdout(predicate::str::contains("🤖"));
}

#[test]
fn test_claim_pool_exhaustion_and_create_and_release_lifecycle() {
    let env = TestGitEnv::setup();

    env.cmd()
        .arg("claim")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "No unclaimed worktree available in the pool",
        ));

    let out = env
        .cmd()
        .args([
            "claim",
            "--create",
            "--print-path",
            "-b",
            "feat/payment",
            "-m",
            "payment-task",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let claimed_path = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    assert!(claimed_path.is_dir());

    env.cmd()
        .arg("claim")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "No unclaimed worktree available in the pool",
        ));

    fs::write(claimed_path.join("dirty.txt"), "uncommitted\n").unwrap();
    env.cmd()
        .args(["release", "feat/payment"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("has uncommitted changes"));

    fs::remove_file(claimed_path.join("dirty.txt")).unwrap();
    env.cmd()
        .args(["release", "feat/payment"])
        .assert()
        .success();

    let head_ref = run_git(&claimed_path, &["rev-parse", "--abbrev-ref", "HEAD"]);
    assert_eq!(head_ref, "HEAD");

    env.cmd()
        .args(["claim", "-m", "reclaimed"])
        .assert()
        .success();
}

#[test]
fn test_detach_preserves_claim_lock() {
    let env = TestGitEnv::setup();

    env.cmd()
        .args(["create", "pinot", "-b", "feat/pinot", "-m", "pinot-owner"])
        .assert()
        .success();

    env.cmd().args(["detach", "feat/pinot"]).assert().success();

    env.cmd()
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("(detached @"))
        .stdout(predicate::str::contains("🔒 claimed (pinot-owner)"));
}

#[test]
fn test_target_resolution_precedence_name_over_branch_over_path() {
    let env = TestGitEnv::setup();

    env.cmd()
        .args(["create", "gamay", "-b", "feat/alpha"])
        .assert()
        .success();

    env.cmd()
        .args(["create", "chenin", "-b", "gamay"])
        .assert()
        .success();

    let gamay_wt_path = env._tmp.path().join("my-repo-worktrees").join("gamay");
    let chenin_wt_path = env._tmp.path().join("my-repo-worktrees").join("chenin");

    let out_name = env
        .cmd()
        .args(["cd", "--shell-eval", "gamay"])
        .output()
        .unwrap();
    assert!(out_name.status.success());
    let stdout_name = String::from_utf8_lossy(&out_name.stdout);
    assert!(
        stdout_name.contains(gamay_wt_path.to_str().unwrap()),
        "expected short_name match {}, got {}",
        gamay_wt_path.display(),
        stdout_name
    );

    let out_branch = env
        .cmd()
        .args(["cd", "--shell-eval", "feat/alpha"])
        .output()
        .unwrap();
    assert!(out_branch.status.success());
    let stdout_branch = String::from_utf8_lossy(&out_branch.stdout);
    assert!(stdout_branch.contains(gamay_wt_path.to_str().unwrap()));

    let out_path = env
        .cmd()
        .args(["cd", "--shell-eval", chenin_wt_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out_path.status.success());
    let stdout_path = String::from_utf8_lossy(&out_path.stdout);
    assert!(stdout_path.contains(chenin_wt_path.to_str().unwrap()));

    env.cmd()
        .args(["claim", "chenin", "-m", "busy"])
        .assert()
        .success();
    env.cmd().args(["remove", "chenin"]).assert().failure();
    env.cmd()
        .args(["remove", "-f", "chenin"])
        .assert()
        .success();
    assert!(!chenin_wt_path.exists());
}

#[test]
fn test_cd_and_cd_dash_history_switching() {
    let env = TestGitEnv::setup();

    env.cmd().args(["create", "malbec"]).assert().success();
    let malbec_path = env._tmp.path().join("my-repo-worktrees").join("malbec");

    let to_malbec = env
        .cmd()
        .args(["cd", "--shell-eval", "malbec"])
        .output()
        .unwrap();
    assert!(to_malbec.status.success());
    let to_malbec_stdout = String::from_utf8_lossy(&to_malbec.stdout);
    assert!(to_malbec_stdout.starts_with("cd '"));
    assert!(to_malbec_stdout.contains(malbec_path.to_str().unwrap()));

    let back_to_main = env
        .cmd_in(&malbec_path)
        .args(["cd", "--shell-eval", "-"])
        .output()
        .unwrap();
    assert!(back_to_main.status.success());
    let back_stdout = String::from_utf8_lossy(&back_to_main.stdout);
    assert!(back_stdout.contains(env.local_dir.to_str().unwrap()));

    let default_main = env
        .cmd_in(&malbec_path)
        .args(["cd", "--shell-eval"])
        .output()
        .unwrap();
    assert!(default_main.status.success());
    let default_stdout = String::from_utf8_lossy(&default_main.stdout);
    assert!(default_stdout.contains(env.local_dir.to_str().unwrap()));
}
