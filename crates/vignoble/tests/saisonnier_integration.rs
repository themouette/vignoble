use assert_cmd::Command;
use git_core::{
    worktree::{add_worktree_detached, get_agent_marker, list_worktrees, lock_worktree},
    GitRunner, WorktreeHead,
};
use predicates::prelude::*;
use saisonnier::store::{RunReceipt, RunStatus, VignobleStore, SCHEMA_VERSION};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use tempfile::{tempdir, TempDir};

struct TestEnv {
    _tmp: TempDir,
    repo_dir: PathBuf,
    saisonnier_bin: PathBuf,
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
        .expect("git execution failed");
    assert!(
        output.status.success(),
        "git {:?} failed in {:?}: {}",
        args,
        cwd,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

impl TestEnv {
    fn setup(ceps: &[&str]) -> Self {
        let tmp = tempdir().unwrap();
        let remote_dir = tmp.path().join("remote.git");
        let repo_dir = tmp.path().join("my-repo");
        fs::create_dir_all(&remote_dir).unwrap();
        fs::create_dir_all(&repo_dir).unwrap();

        run_git(&remote_dir, &["init", "--bare", "--initial-branch=main"]);
        run_git(&repo_dir, &["init", "--initial-branch=main"]);
        run_git(&repo_dir, &["config", "user.name", "Test User"]);
        run_git(&repo_dir, &["config", "user.email", "test@example.com"]);
        run_git(
            &repo_dir,
            &["remote", "add", "origin", remote_dir.to_str().unwrap()],
        );

        fs::write(repo_dir.join("README.md"), "initial\n").unwrap();
        run_git(&repo_dir, &["add", "README.md"]);
        run_git(&repo_dir, &["commit", "-m", "Initial commit"]);
        run_git(&repo_dir, &["push", "-u", "origin", "main"]);

        let git = GitRunner::new(&repo_dir, false);
        let pool_dir = tmp.path().join("my-repo-worktrees");
        fs::create_dir_all(&pool_dir).unwrap();
        for name in ceps {
            let wt_path = pool_dir.join(name);
            add_worktree_detached(&git, &wt_path, Some("origin/main")).unwrap();
        }

        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let vignoble_src = assert_cmd::cargo::cargo_bin!("vignoble");
        let saisonnier_bin = bin_dir.join(format!("saisonnier{}", std::env::consts::EXE_SUFFIX));
        #[cfg(unix)]
        std::os::unix::fs::symlink(vignoble_src, &saisonnier_bin).unwrap();
        #[cfg(windows)]
        fs::copy(vignoble_src, &saisonnier_bin).unwrap();

        Self {
            _tmp: tmp,
            repo_dir,
            saisonnier_bin,
        }
    }

    fn write_config(&self, toml_content: &str) {
        fs::write(self.repo_dir.join(".vignoble.toml"), toml_content).unwrap();
    }

    fn cmd(&self) -> Command {
        self.cmd_in(&self.repo_dir)
    }

    fn cmd_in(&self, dir: &Path) -> Command {
        let mut cmd = Command::new(&self.saisonnier_bin);
        cmd.current_dir(dir);
        cmd.env("HOME", self._tmp.path());
        cmd.env("GIT_CONFIG_NOSYSTEM", "1");
        cmd.env("GIT_CONFIG_GLOBAL", "/dev/null");
        cmd
    }

    fn git(&self) -> GitRunner {
        GitRunner::new(&self.repo_dir, false)
    }

    fn store(&self) -> VignobleStore {
        let common = self.git().git_common_dir().unwrap();
        VignobleStore::from_git_common_dir(&common)
    }

    fn cep_path(&self, name: &str) -> PathBuf {
        self._tmp.path().join("my-repo-worktrees").join(name)
    }
}

#[test]
fn test_clean_run_commits_auto_releases_and_drains_next_queued_task() {
    let env = TestEnv::setup(&["syrah"]);

    env.write_config(
        r#"
[saisonnier]
default_profile = "worker"
branch_template = "auto/{slug}-{run_id}"

[saisonnier.profiles.worker]
run = '''
set -eu
test "$VIGNOBLE_SAISONNIER" = "1"
test -f "$SAISONNIER_PROMPT_FILE"
echo "starting run $SAISONNIER_RUN_ID"
echo "prompt: $SAISONNIER_PROMPT" > work-$SAISONNIER_RUN_ID.txt
echo "{\"event\":\"done\"}" > "{session_path}"
git add work-$SAISONNIER_RUN_ID.txt
git commit -m "Commit for $SAISONNIER_RUN_ID"
echo "finished $SAISONNIER_RUN_ID"
'''
resume = "echo resumed {session_path} {session_id}"
"#,
    );

    // Enqueue two tasks onto a single-cep pool without ticking yet.
    env.cmd()
        .args([
            "enqueue",
            "--no-tick",
            "-b",
            "feat/first-task",
            "Implement first feature",
        ])
        .assert()
        .success();

    env.cmd()
        .args(["enqueue", "--no-tick", "Add second feature"])
        .assert()
        .success();

    assert_eq!(env.store().list_tasks().unwrap().len(), 2);

    // Drain the queue inline: task 1 commits & auto-releases syrah, which immediately picks up task 2.
    env.cmd().args(["tick", "--inline"]).assert().success();

    assert!(
        env.store().list_tasks().unwrap().is_empty(),
        "expected queue to be completely drained"
    );

    let receipts = env.store().list_receipts().unwrap();
    assert_eq!(receipts.len(), 2);
    for r in &receipts {
        assert_eq!(r.status, RunStatus::Completed);
        assert_eq!(r.exit_code, Some(0));
        assert_eq!(r.cep, "syrah");
        assert!(r.end_sha.is_some());
        assert_ne!(Some(r.start_sha.as_str()), r.end_sha.as_deref());
        assert!(r.prompt_file.is_file());
        assert!(r.log_path.is_file());
        assert!(r.session_path.is_file());
        assert!(r.summary.as_deref().unwrap().contains("finished"));
    }

    let branches: Vec<&str> = receipts.iter().map(|r| r.branch.as_str()).collect();
    assert!(branches.contains(&"feat/first-task"));
    assert!(branches
        .iter()
        .any(|b| b.starts_with("auto/add-second-feature-")));

    // Verify syrah was auto-released (unlocked, detached, marker cleared).
    let worktrees = list_worktrees(&env.git()).unwrap();
    let syrah = worktrees.iter().find(|w| w.short_name == "syrah").unwrap();
    assert!(syrah.locked.is_none());
    assert_eq!(syrah.head, WorktreeHead::Detached);
    assert_eq!(get_agent_marker(&env.git(), "syrah"), None);
}

#[test]
fn test_zero_commit_run_auto_releases_cep_and_records_no_changes_receipt() {
    let env = TestEnv::setup(&["syrah"]);

    // Create a local branch `themouette/saisonnier` to verify that a branch_template
    // expanding to `themouette/saisonnier/{run_id}` rewrites the colliding `/` to `-`.
    run_git(&env.repo_dir, &["branch", "themouette/saisonnier"]);

    env.write_config(
        r#"
[saisonnier]
default_profile = "thinker"
branch_template = "themouette/saisonnier/{run_id}"

[saisonnier.profiles.thinker]
run = '''
echo "Need clarification from user"
exit 0
'''
resume = "echo resume {session_id}"
"#,
    );

    env.cmd()
        .env("SAISONNIER_RUNNER", "inline")
        .args(["enqueue", "Investigate flaky test"])
        .assert()
        .success();

    let receipts = env.store().list_receipts().unwrap();
    assert_eq!(receipts.len(), 1);
    let r = &receipts[0];
    assert_eq!(r.status, RunStatus::Completed);
    assert_eq!(r.exit_code, Some(0));
    assert_eq!(r.end_sha.as_deref(), Some(r.start_sha.as_str()));
    assert_eq!(r.branch, format!("themouette/saisonnier-{}", r.run_id));

    let worktrees = list_worktrees(&env.git()).unwrap();
    let syrah = worktrees.iter().find(|w| w.short_name == "syrah").unwrap();
    assert!(
        syrah.locked.is_none(),
        "expected syrah to be auto-released after clean zero-commit run"
    );
    assert_eq!(syrah.head, WorktreeHead::Detached);
    assert_eq!(get_agent_marker(&env.git(), "syrah"), None);
}

#[test]
fn test_failed_and_dirty_runs_preserve_ceps_and_halt_queue_when_pool_full() {
    let env = TestEnv::setup(&["syrah", "merlot"]);

    env.write_config(
        r#"
[saisonnier]
default_profile = "failer"

[saisonnier.profiles.failer]
run = '''
echo "boom on stderr" >&2
exit 2
'''

[saisonnier.profiles.dirty]
run = '''
echo "uncommitted work" > leftover.txt
echo "done without committing"
exit 0
'''
"#,
    );

    env.cmd()
        .args(["enqueue", "--no-tick", "--profile", "failer", "Task 1 fail"])
        .assert()
        .success();
    env.cmd()
        .args(["enqueue", "--no-tick", "--profile", "dirty", "Task 2 dirty"])
        .assert()
        .success();
    env.cmd()
        .args([
            "enqueue",
            "--no-tick",
            "--profile",
            "failer",
            "Task 3 blocked",
        ])
        .assert()
        .success();

    env.cmd().args(["tick", "--inline"]).assert().success();

    // Task 3 must stay queued because both ceps are preserved and tick never auto-creates worktrees.
    let remaining = env.store().list_tasks().unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].prompt, "Task 3 blocked");

    let receipts = env.store().list_receipts().unwrap();
    assert_eq!(receipts.len(), 2);
    assert!(receipts
        .iter()
        .any(|r| r.status == RunStatus::Failed && r.exit_code == Some(2)));
    assert!(receipts
        .iter()
        .any(|r| r.status == RunStatus::Dirty && r.exit_code == Some(0)));

    let worktrees = list_worktrees(&env.git()).unwrap();
    // Main + 2 pool ceps = 3 total (no extra worktree created).
    assert_eq!(worktrees.len(), 3);
    for name in ["syrah", "merlot"] {
        let wt = worktrees.iter().find(|w| w.short_name == name).unwrap();
        assert!(wt.locked.is_some());
        assert_eq!(get_agent_marker(&env.git(), name).as_deref(), Some("⚠️"));
    }

    // Verify `saisonnier list --json` output structure.
    let list_out = env
        .cmd()
        .args(["list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&list_out).unwrap();
    assert_eq!(json["queued"].as_array().unwrap().len(), 1);
    assert_eq!(json["active"].as_array().unwrap().len(), 0);
    assert_eq!(json["recent"].as_array().unwrap().len(), 2);
}

#[test]
fn test_list_inspect_cancel_and_crash_reconciliation() {
    let env = TestEnv::setup(&["syrah"]);
    env.write_config(
        r#"
[saisonnier]
default_profile = "logger"

[saisonnier.profiles.logger]
run = '''
echo "hello" > out.txt
git add out.txt
git commit -q -m "add out"
echo "line 1"
echo "line 2"
echo "line 3"
echo "line 4"
'''
"#,
    );

    // 1. Enqueue without tick, inspect queued task, then cancel it.
    env.cmd()
        .args(["enqueue", "--no-tick", "Queued task to cancel"])
        .assert()
        .success();

    let queued = env.store().list_tasks().unwrap();
    assert_eq!(queued.len(), 1);
    let task_id = queued[0].task_id.clone();

    env.cmd()
        .args(["inspect", &task_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Queued task to cancel"))
        .stdout(predicate::str::contains("queued"));

    env.cmd()
        .args(["cancel", &task_id])
        .assert()
        .success()
        .stdout(predicate::str::contains(&task_id));
    assert!(env.store().list_tasks().unwrap().is_empty());

    // 2. Run a completed task and inspect receipt + tail lines.
    env.cmd()
        .env("SAISONNIER_RUNNER", "inline")
        .args(["enqueue", "Write output file"])
        .assert()
        .success();
    let run_id = env.store().list_receipts().unwrap()[0].run_id.clone();

    env.cmd()
        .args(["inspect", &run_id, "-n", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&run_id))
        .stdout(predicate::str::contains("Write output file"))
        .stdout(predicate::str::contains("line 3"))
        .stdout(predicate::str::contains("line 4"))
        .stdout(predicate::str::contains("line 1").not());

    // 3. Crash reconciliation: a running receipt with a dead tmux session becomes `crashed` + ⚠️.
    let crashed_run_id = "20261127-000001-dead01".to_string();
    let crashed_receipt = RunReceipt {
        schema_version: SCHEMA_VERSION,
        run_id: crashed_run_id.clone(),
        task_id: "20261127-000000-dead00".to_string(),
        cep: "syrah".to_string(),
        branch: "saisonnier/dead".to_string(),
        base: Some("origin/main".to_string()),
        start_sha: "0000000000000000000000000000000000000000".to_string(),
        end_sha: None,
        profile: "logger".to_string(),
        prompt_file: env.store().prompt_path(&crashed_run_id),
        log_path: env.store().log_path(&crashed_run_id),
        session_path: env.store().session_path(&crashed_run_id),
        session_id: "00000000-0000-4000-8000-000000000001".to_string(),
        tmux_session: "nonexistent-vignoble-test-session/syrah".to_string(),
        started_at: "2026-11-27T00:00:01Z".to_string(),
        finished_at: None,
        exit_code: None,
        status: RunStatus::Running,
        summary: None,
    };
    env.store().write_receipt(&crashed_receipt).unwrap();

    env.cmd()
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("crashed"))
        .stdout(predicate::str::contains(&crashed_run_id));

    let updated = env.store().read_receipt(&crashed_run_id).unwrap();
    assert_eq!(updated.status, RunStatus::Crashed);
    assert!(updated.finished_at.is_some());
    assert_eq!(get_agent_marker(&env.git(), "syrah").as_deref(), Some("⚠️"));

    // 4. Cancel a running inline receipt with `--release` on a clean worktree.
    let active_run_id = "20261127-000002-live01".to_string();
    lock_worktree(&env.git(), &env.cep_path("syrah"), Some("saisonnier:live")).unwrap();
    let active_receipt = RunReceipt {
        schema_version: SCHEMA_VERSION,
        run_id: active_run_id.clone(),
        task_id: "20261127-000002-live00".to_string(),
        cep: "syrah".to_string(),
        branch: "saisonnier/live".to_string(),
        base: Some("origin/main".to_string()),
        start_sha: "0000000000000000000000000000000000000000".to_string(),
        end_sha: None,
        profile: "logger".to_string(),
        prompt_file: env.store().prompt_path(&active_run_id),
        log_path: env.store().log_path(&active_run_id),
        session_path: env.store().session_path(&active_run_id),
        session_id: "00000000-0000-4000-8000-000000000002".to_string(),
        tmux_session: String::new(),
        started_at: "2026-11-27T00:00:02Z".to_string(),
        finished_at: None,
        exit_code: None,
        status: RunStatus::Running,
        summary: None,
    };
    env.store().write_receipt(&active_receipt).unwrap();

    env.cmd()
        .args(["cancel", "--release", &active_run_id])
        .assert()
        .success();

    let cancelled = env.store().read_receipt(&active_run_id).unwrap();
    assert_eq!(cancelled.status, RunStatus::Cancelled);
    let syrah = list_worktrees(&env.git())
        .unwrap()
        .into_iter()
        .find(|w| w.short_name == "syrah")
        .unwrap();
    assert!(syrah.locked.is_none());
    assert_eq!(get_agent_marker(&env.git(), "syrah"), None);
}

#[test]
fn test_resume_reuses_claimed_cep_and_claims_free_cep_after_auto_release() {
    let env = TestEnv::setup(&["syrah"]);
    let marker_file = env._tmp.path().join("resume-out.txt");

    env.write_config(&format!(
        r#"
[saisonnier]
default_profile = "noop"

[saisonnier.profiles.noop]
run = "echo nothing committed"
resume = "printf '%s|%s|%s\n' '{{cep}}' '{{branch}}' '{{session_id}}' > '{}'"

[saisonnier.profiles.committer]
run = '''
echo "ok" > done.txt
git add done.txt
git commit -m "done"
'''
resume = "printf '%s|%s|%s\n' '{{cep}}' '{{branch}}' '{{session_id}}' > '{}'"
"#,
        marker_file.display(),
        marker_file.display(),
    ));

    // Case 1: zero-commit completed run auto-releases syrah; resume claims syrah and checks out its branch.
    env.cmd()
        .env("SAISONNIER_RUNNER", "inline")
        .args(["enqueue", "-b", "feat/needs-input", "Ask question"])
        .assert()
        .success();
    let noop_run = env.store().list_receipts().unwrap()[0].clone();
    assert_eq!(noop_run.status, RunStatus::Completed);

    env.cmd()
        .args(["resume", &noop_run.run_id])
        .assert()
        .success();
    let resume_output = fs::read_to_string(&marker_file).unwrap();
    assert_eq!(
        resume_output.trim(),
        format!("syrah|feat/needs-input|{}", noop_run.session_id)
    );

    // Release syrah so Case 2 can run a completed task that auto-releases syrah.
    run_git(&env.cep_path("syrah"), &["checkout", "--detach"]);
    run_git(
        &env.repo_dir,
        &[
            "worktree",
            "unlock",
            env.cep_path("syrah").to_str().unwrap(),
        ],
    );

    // Case 2: completed run auto-releases syrah; resume claims free syrah and checks out its branch.
    env.cmd()
        .env("SAISONNIER_RUNNER", "inline")
        .args([
            "enqueue",
            "--profile",
            "committer",
            "-b",
            "feat/completed-work",
            "Complete work",
        ])
        .assert()
        .success();
    let completed_run = env
        .store()
        .list_receipts()
        .unwrap()
        .into_iter()
        .find(|r| r.branch == "feat/completed-work")
        .unwrap();
    assert_eq!(completed_run.status, RunStatus::Completed);

    env.cmd()
        .args(["resume", &completed_run.run_id])
        .assert()
        .success();
    let resume_output_2 = fs::read_to_string(&marker_file).unwrap();
    assert_eq!(
        resume_output_2.trim(),
        format!("syrah|feat/completed-work|{}", completed_run.session_id)
    );

    let current_branch = run_git(
        &env.cep_path("syrah"),
        &["rev-parse", "--abbrev-ref", "HEAD"],
    );
    assert_eq!(current_branch, "feat/completed-work");
}

#[test]
fn test_enqueue_supports_file_and_piped_stdin_and_works_from_linked_worktree() {
    let env = TestEnv::setup(&["syrah"]);

    let prompt_path = env._tmp.path().join("task.md");
    fs::write(&prompt_path, "Prompt loaded from file\nSecond line\n").unwrap();

    env.cmd()
        .args(["enqueue", "--no-tick", "-f", prompt_path.to_str().unwrap()])
        .assert()
        .success();

    env.cmd_in(&env.cep_path("syrah"))
        .args(["enqueue", "--no-tick"])
        .write_stdin("Prompt piped via stdin from worktree\n")
        .assert()
        .success();

    let tasks = env.store().list_tasks().unwrap();
    assert_eq!(tasks.len(), 2);
    assert!(tasks[0].prompt.contains("Prompt loaded from file"));
    assert!(tasks[1]
        .prompt
        .contains("Prompt piped via stdin from worktree"));
}

#[test]
fn test_edit_logs_clean_and_grouped_help() {
    let env = TestEnv::setup(&["syrah"]);
    env.write_config(
        r#"
[saisonnier]
default_profile = "noop"

[saisonnier.profiles.noop]
run = 'echo "line 1"; echo "line 2"; echo "line 3"'

[saisonnier.profiles.committer]
run = 'echo "$SAISONNIER_PROMPT" > out.txt && git add out.txt && git commit -m "Commit from saisonnier"'
"#,
    );

    // 1. Grouped help displays both sections and hides internal exec.
    env.cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Task & Queue Management:"))
        .stdout(predicate::str::contains("Worker & Run Execution:"))
        .stdout(predicate::str::contains("exec").not());

    // 2. Enqueue two tasks without ticking, then edit the second task via flags and $EDITOR.
    env.cmd()
        .args(["enqueue", "--no-tick", "-b", "feat/noop", "First noop task"])
        .assert()
        .success();
    env.cmd()
        .args(["enqueue", "--no-tick", "Initial second prompt"])
        .assert()
        .success();

    let tasks = env.store().list_tasks().unwrap();
    assert_eq!(tasks.len(), 2);
    let second_id = tasks[1].task_id.clone();

    env.cmd()
        .args([
            "edit",
            &second_id,
            "-b",
            "feat/edited-branch",
            "--profile",
            "committer",
            "-p",
            "Updated via flags",
        ])
        .assert()
        .success();

    let edited = env.store().find_task(&second_id).unwrap().unwrap();
    assert_eq!(edited.branch.as_deref(), Some("feat/edited-branch"));
    assert_eq!(edited.profile.as_deref(), Some("committer"));
    assert_eq!(edited.prompt, "Updated via flags");

    // Edit via $EDITOR script.
    let editor_script = env._tmp.path().join("fake-editor.sh");
    fs::write(
        &editor_script,
        "#!/bin/sh\necho 'Edited by fake EDITOR' > \"$1\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&editor_script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&editor_script, perms).unwrap();
    }

    env.cmd()
        .env("EDITOR", &editor_script)
        .env_remove("VISUAL")
        .args(["edit", &second_id])
        .assert()
        .success();

    let edited_by_editor = env.store().find_task(&second_id).unwrap().unwrap();
    assert_eq!(edited_by_editor.prompt.trim(), "Edited by fake EDITOR");

    // 3. Run tick: first task (noop) runs on syrah, finishes with no_changes (💬), auto-releases syrah,
    //    and immediately drains the second edited task!
    env.cmd()
        .env("SAISONNIER_RUNNER", "inline")
        .args(["tick"])
        .assert()
        .success();

    assert!(env.store().list_tasks().unwrap().is_empty());

    let receipts = env.store().list_receipts().unwrap();
    assert_eq!(receipts.len(), 2);
    let noop_run = receipts
        .iter()
        .find(|r| r.branch == "feat/noop")
        .unwrap()
        .clone();
    assert_eq!(noop_run.status, RunStatus::Completed);

    // 4. `saisonnier logs` prints full log or last N lines.
    env.cmd()
        .args(["logs", &noop_run.run_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("line 1\nline 2\nline 3"));

    env.cmd()
        .args(["logs", "-n", "1", &noop_run.run_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("line 1").not())
        .stdout(predicate::str::contains("line 3"));

    // 5. `saisonnier clean <run-id>` removes the no_changes receipt while keeping the completed receipt.
    env.cmd()
        .args(["clean", &noop_run.run_id])
        .assert()
        .success();

    assert!(env
        .store()
        .find_receipt(&noop_run.run_id)
        .unwrap()
        .is_none());

    let remaining_receipts = env.store().list_receipts().unwrap();
    assert_eq!(remaining_receipts.len(), 1);
    assert_eq!(remaining_receipts[0].branch, "feat/edited-branch");
    assert_eq!(remaining_receipts[0].status, RunStatus::Completed);
    assert_eq!(get_agent_marker(&env.git(), "syrah"), None);
}
