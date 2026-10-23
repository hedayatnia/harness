//! End-to-end: a checkout hook that calls a tool found only on the login
//! shell's PATH runs when the engine creates a worktree.
//!
//! This file must stay a single test: it mutates process env (SHELL/PATH/HOME)
//! and warms the process-global login-shell snapshot cache, so it needs its
//! own test binary with no parallel siblings.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use harness_engine::Repos;

fn write_executable(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn run_git(cwd: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=Harness Test",
            "-c",
            "user.email=test@example.com",
        ])
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("git fixture command starts");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn worktree_checkout_hook_finds_login_shell_tools() {
    let dir = tempfile::tempdir().unwrap();
    let direct_bin = dir.path().join("direct-bin");
    std::fs::create_dir(&direct_bin).unwrap();
    let git_executable = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|path| path.join("git"))
        .find(|path| path.is_file())
        .expect("git available for the fixture");
    std::os::unix::fs::symlink(&git_executable, direct_bin.join("git")).unwrap();

    // A user tool (an `npx` stand-in) only the login shell puts on PATH.
    let shell_bin = dir.path().join("shell-bin");
    std::fs::create_dir(&shell_bin).unwrap();
    let ran = dir.path().join("hook-tool-ran");
    write_executable(
        &shell_bin.join("hook-tool"),
        &format!("#!/bin/sh\n: > \"{}\"\n", ran.display()),
    );

    let fake_shell = dir.path().join("fake-shell");
    write_executable(
        &fake_shell,
        &format!(
            "#!/bin/sh\nPATH=\"{}:/usr/bin:/bin\"; export PATH\n\
             while [ \"$#\" -gt 0 ]; do\n\
               if [ \"$1\" = \"-c\" ]; then shift; exec /bin/sh -c \"$1\"; fi\n\
               shift\n\
             done\nexit 1\n",
            shell_bin.display()
        ),
    );

    let checkout = dir.path().join("checkout");
    std::fs::create_dir(&checkout).unwrap();
    run_git(&checkout, &["init", "-q", "-b", "main"]);
    run_git(&checkout, &["commit", "-q", "--allow-empty", "-m", "init"]);
    write_executable(
        &checkout.join(".git").join("hooks").join("post-checkout"),
        "#!/bin/sh\nhook-tool\n",
    );

    // A GUI/service-launch environment: only git is available directly; the
    // user's shell adds the hook's tool through its login PATH.
    // SAFETY: single-test binary — nothing else reads env concurrently.
    unsafe {
        std::env::set_var("SHELL", &fake_shell);
        std::env::set_var("HOME", dir.path());
        std::env::set_var("PATH", &direct_bin);
        std::env::remove_var("HARNESS_NO_LOGIN_SHELL");
    }

    let data_dir = dir.path().join("data");
    std::fs::create_dir(&data_dir).unwrap();
    let repos = Repos::with_worktrees_root(&data_dir, "device-test", dir.path().join("worktrees"));
    let worktree = repos
        .create_worktree(&checkout, "main")
        .await
        .expect("worktree created with its checkout hook passing");
    assert!(Path::new(&worktree.path).is_dir());
    assert!(ran.exists(), "the checkout hook ran its login-shell tool");
}
