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
fn piggy_create_fund_and_status_shows_progress() {
    let home = tempfile::tempdir().expect("tempdir");

    run_ok(
        &home,
        &[
            "piggy",
            "create",
            "New Car",
            "5000",
            "USD",
            "--from",
            "assets:savings",
        ],
    );

    run_ok(&home, &["piggy", "fund", "New Car", "2000", "USD"]);

    let out = run_ok_out(&home, &["piggy", "status", "New Car"]);
    assert!(out.contains("40%"), "status output: {out}");
    assert!(out.contains("2000"), "status output: {out}");
    assert!(out.contains("5000"), "status output: {out}");
    assert!(out.contains("remaining\tUSD\t3000"), "status output: {out}");
}

#[test]
fn balance_shows_reserved_piggies_and_effective_balance() {
    let home = tempfile::tempdir().expect("tempdir");

    let t = "2026-02-25T12:00:00Z";

    run_ok(
        &home,
        &[
            "deposit",
            "3000",
            "USD",
            "--to",
            "assets:savings",
            "--from",
            "income:salary",
            "--effective-at",
            t,
        ],
    );

    run_ok(
        &home,
        &[
            "piggy",
            "create",
            "New Car",
            "5000",
            "USD",
            "--from",
            "assets:savings",
        ],
    );

    run_ok(
        &home,
        &[
            "piggy",
            "fund",
            "New Car",
            "2000",
            "USD",
            "--effective-at",
            t,
        ],
    );

    let out = run_ok_out(&home, &["balance", "assets:savings"]);
    assert!(
        out.contains("assets:savings\tUSD\t3000"),
        "balance output: {out}"
    );
    assert!(out.contains("(reserved piggies)"), "balance output: {out}");
    assert!(
        out.contains("assets:savings\tUSD\t-2000"),
        "balance output: {out}"
    );
    assert!(out.contains("(effective balance)"), "balance output: {out}");
    assert!(
        out.contains("assets:savings\tUSD\t1000"),
        "balance output: {out}"
    );
}
