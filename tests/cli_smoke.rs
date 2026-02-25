use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

fn bankero_cmd() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("bankero"))
}

fn cmd_with_home() -> (tempfile::TempDir, Command) {
    let home = tempfile::tempdir().expect("tempdir");
    let mut cmd = bankero_cmd();
    cmd.env("BANKERO_HOME", home.path());
    (home, cmd)
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
fn deposit_and_move_write_events_and_balance_rebuilds() {
    let (home, _cmd) = cmd_with_home();

    // Freeze event time so month/range filtering is deterministic.
    let t = "2026-02-25T12:00:00Z";

    run_ok(
        &home,
        &[
            "deposit",
            "1500",
            "USD",
            "--to",
            "assets:savings",
            "--from",
            "income:freelance",
            "-m",
            "Web project payout",
            "--category",
            "income:freelance",
            "--tag",
            "client:acme",
            "--effective-at",
            t,
        ],
    );

    run_ok(
        &home,
        &[
            "move",
            "100",
            "USD",
            "--from",
            "assets:wells-fargo",
            "--to",
            "assets:banesco",
            "42000",
            "VES",
            "@manual:420",
            "--effective-at",
            t,
        ],
    );

    let out = run_ok_out(&home, &["balance"]);

    // Output uses tabs; just assert key lines exist.
    assert!(out.contains("assets:savings\tUSD\t1500"));
    assert!(out.contains("income:freelance\tUSD\t-1500"));
    assert!(out.contains("assets:banesco\tVES\t42000"));
    assert!(out.contains("assets:wells-fargo\tUSD\t-100"));
}

#[test]
fn report_filters_by_month_category_and_tag() {
    let (home, _cmd) = cmd_with_home();

    let t = "2026-02-25T12:00:00Z";

    run_ok(
        &home,
        &[
            "deposit",
            "1500",
            "USD",
            "--to",
            "assets:savings",
            "--from",
            "income:freelance",
            "--category",
            "income:freelance",
            "--tag",
            "client:acme",
            "--effective-at",
            t,
        ],
    );

    run_ok(
        &home,
        &[
            "move",
            "100",
            "USD",
            "--from",
            "assets:wells-fargo",
            "--to",
            "assets:banesco",
            "42000",
            "VES",
            "@manual:420",
            "--effective-at",
            t,
        ],
    );

    let out_all = run_ok_out(&home, &["report", "--month", "2026-02"]);
    assert!(out_all.contains("\tdeposit\t"));
    assert!(out_all.contains("\tmove\t"));

    let out_cat = run_ok_out(
        &home,
        &["report", "--month", "2026-02", "--category", "income:freelance"],
    );
    assert!(out_cat.contains("\tdeposit\t"));
    assert!(!out_cat.contains("\tmove\t"));

    let out_tag = run_ok_out(
        &home,
        &["report", "--month", "2026-02", "--tag", "client:acme"],
    );
    assert!(out_tag.contains("\tdeposit\t"));
    assert!(!out_tag.contains("\tmove\t"));
}

#[test]
fn buy_with_splits_requires_sum_match() {
    let (home, _cmd) = cmd_with_home();

    // NOTE: current CLI requires a payee/target argument before amount.
    run_ok(
        &home,
        &[
            "buy",
            "external:utilities",
            "500",
            "USD",
            "--from",
            "assets:bank",
            "--to",
            "expenses:rent:450",
            "--to",
            "expenses:water:50",
            "--effective-at",
            "2026-02-25T12:00:00Z",
        ],
    );

    let out = run_ok_out(&home, &["balance", "expenses:"]);
    assert!(out.contains("expenses:rent\tUSD\t450"));
    assert!(out.contains("expenses:water\tUSD\t50"));

    // Now a mismatched split should fail.
    let mut cmd = bankero_cmd();
    cmd.env("BANKERO_HOME", home.path());
    cmd.args([
        "buy",
        "external:utilities",
        "500",
        "USD",
        "--from",
        "assets:bank",
        "--to",
        "expenses:rent:400",
        "--to",
        "expenses:water:50",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Split amounts must sum"));
}
