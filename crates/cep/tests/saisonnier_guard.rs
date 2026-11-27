use cep::commands::{
    claim::{self, ClaimArgs},
    detach::{self, DetachArgs},
    release::{self, ReleaseArgs},
    ExecutionContext, SAISONNIER_ENV_VAR, SAISONNIER_INTERNAL_ENV_VAR,
};
use cep::config::EffectiveConfig;
use git_core::{
    worktree::{add_worktree_detached, lock_worktree},
    GitRunner,
};
use std::fs;
use std::path::Path;
use std::process::Command as StdCommand;
use std::sync::Mutex;
use tempfile::tempdir;

static ENV_MUTEX: Mutex<()> = Mutex::new(());

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
        .expect("git execution failed");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

struct EnvGuard;

impl EnvGuard {
    fn set(managed: Option<&str>, internal: Option<&str>) -> Self {
        match managed {
            Some(v) => std::env::set_var(SAISONNIER_ENV_VAR, v),
            None => std::env::remove_var(SAISONNIER_ENV_VAR),
        }
        match internal {
            Some(v) => std::env::set_var(SAISONNIER_INTERNAL_ENV_VAR, v),
            None => std::env::remove_var(SAISONNIER_INTERNAL_ENV_VAR),
        }
        Self
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        std::env::remove_var(SAISONNIER_ENV_VAR);
        std::env::remove_var(SAISONNIER_INTERNAL_ENV_VAR);
    }
}

#[test]
fn test_saisonnier_guard_blocks_claim_detach_release_and_allows_internal_override() {
    let _lock = ENV_MUTEX.lock().unwrap();

    let tmp = tempdir().unwrap();
    let repo_dir = tmp.path().join("my-repo");
    fs::create_dir_all(&repo_dir).unwrap();
    run_git(&repo_dir, &["init", "-b", "main"]);
    fs::write(repo_dir.join("README.md"), "hello\n").unwrap();
    run_git(&repo_dir, &["add", "README.md"]);
    run_git(&repo_dir, &["commit", "-m", "initial"]);

    let wt_dir = tmp.path().join("my-repo-worktrees").join("syrah");
    fs::create_dir_all(wt_dir.parent().unwrap()).unwrap();
    let git = GitRunner::new(&repo_dir, false);
    add_worktree_detached(&git, &wt_dir, Some("main")).unwrap();

    let ctx = ExecutionContext {
        git: GitRunner::new(&repo_dir, false),
        main_repo_root: repo_dir.clone(),
        config: EffectiveConfig::default(),
    };

    // When VIGNOBLE_SAISONNIER=1 is set, claim, detach, and release all fail with clear errors.
    {
        let _guard = EnvGuard::set(Some("1"), None);

        let claim_err = claim::execute(
            &ctx,
            &ClaimArgs {
                target: Some("syrah".to_string()),
                branch: Some("feat/blocked".to_string()),
                base: None,
                reason: None,
                create: false,
                force: false,
                print_path: false,
                no_cd: true,
                shell_eval: false,
            },
        )
        .unwrap_err()
        .to_string();
        assert!(
            claim_err.contains("Cannot run 'cep claim' inside a managed saisonnier session (VIGNOBLE_SAISONNIER=1)"),
            "unexpected claim error: {}",
            claim_err
        );

        let detach_err = detach::execute(
            &ctx,
            &DetachArgs {
                target: Some("syrah".to_string()),
            },
        )
        .unwrap_err()
        .to_string();
        assert!(
            detach_err.contains("Cannot run 'cep detach' inside a managed saisonnier session (VIGNOBLE_SAISONNIER=1)"),
            "unexpected detach error: {}",
            detach_err
        );

        let release_err = release::execute(
            &ctx,
            &ReleaseArgs {
                target: Some("syrah".to_string()),
                keep_branch: false,
                force: false,
            },
        )
        .unwrap_err()
        .to_string();
        assert!(
            release_err.contains("Cannot run 'cep release' inside a managed saisonnier session (VIGNOBLE_SAISONNIER=1)"),
            "unexpected release error: {}",
            release_err
        );
    }

    // With VIGNOBLE_SAISONNIER_INTERNAL=1 set alongside VIGNOBLE_SAISONNIER=1, lifecycle commands succeed.
    {
        let _guard = EnvGuard::set(Some("1"), Some("1"));

        lock_worktree(&ctx.git, &wt_dir, Some("internal-test")).unwrap();
        detach::execute(
            &ctx,
            &DetachArgs {
                target: Some("syrah".to_string()),
            },
        )
        .unwrap();
        release::execute(
            &ctx,
            &ReleaseArgs {
                target: Some("syrah".to_string()),
                keep_branch: false,
                force: false,
            },
        )
        .unwrap();
        claim::execute(
            &ctx,
            &ClaimArgs {
                target: Some("syrah".to_string()),
                branch: Some("feat/allowed".to_string()),
                base: Some("main".to_string()),
                reason: Some("internal-override".to_string()),
                create: false,
                force: false,
                print_path: true,
                no_cd: true,
                shell_eval: false,
            },
        )
        .unwrap();
    }
}
