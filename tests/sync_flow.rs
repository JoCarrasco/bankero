use assert_cmd::prelude::*;
use std::process::Command;

fn bankero_cmd() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("bankero"))
}

fn run_ok(home: &tempfile::TempDir, args: &[&str]) {
    let mut cmd = bankero_cmd();
    cmd.env("BANKERO_HOME", home.path());
    cmd.args(args);
    cmd.assert().success();
}

fn run_ok_out(home: &tempfile::TempDir, args: &[&str]) -> String {
    let mut cmd = bankero_cmd();
    cmd.env("BANKERO_HOME", home.path());
    cmd.args(args);
    let out = cmd.assert().success().get_output().stdout.clone();
    String::from_utf8(out).expect("utf8 stdout")
}

#[test]
fn sync_transfers_events_between_two_homes() {
    let home_a = tempfile::tempdir().expect("tempdir home_a");
    let home_b = tempfile::tempdir().expect("tempdir home_b");
    let sync_dir = tempfile::tempdir().expect("tempdir sync_dir");

    println!("[sync_flow] configuring shared sync dir");

    // Configure both devices to use the same shared folder.
    run_ok(
        &home_a,
        &[
            "login",
            "--sync-dir",
            sync_dir.path().to_str().expect("utf8 path"),
        ],
    );
    run_ok(
        &home_b,
        &[
            "login",
            "--sync-dir",
            sync_dir.path().to_str().expect("utf8 path"),
        ],
    );

    // Write an event on A.
    println!("[sync_flow] writing deposit event on device A");
    run_ok(
        &home_a,
        &[
            "deposit",
            "100",
            "USD",
            "--to",
            "assets:cash",
            "--from",
            "income:salary",
            "--effective-at",
            "2026-02-25T12:00:00Z",
        ],
    );

    // Sync A -> folder, then B imports from folder.
    println!("[sync_flow] syncing device A -> shared folder");
    run_ok(&home_a, &["sync", "now"]);
    println!("[sync_flow] syncing device B <- shared folder");
    run_ok(&home_b, &["sync", "now"]);
    println!("[sync_flow] verifying balance on device B");

    let out = run_ok_out(&home_b, &["balance", "assets:cash"]);
    assert!(
        out.contains("assets:cash\tUSD\t100"),
        "balance output: {out}"
    );

    println!("[sync_flow] complete");
}
