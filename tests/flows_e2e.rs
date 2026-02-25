use assert_cmd::Command;
use predicates::prelude::*;

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
fn workspace_isolation_applies_to_events_and_rates() {
    let home = tempfile::tempdir().expect("tempdir");
    let t = "2026-02-25T12:00:00Z";

    // In the default workspace ("personal"): store a rate + write an event.
    run_ok(
        &home,
        &["rate", "set", "@bcv", "USD", "VES", "45.2", "--as-of", t],
    );
    run_ok(
        &home,
        &[
            "deposit",
            "100",
            "USD",
            "--to",
            "assets:usd",
            "--from",
            "income:salary",
            "--effective-at",
            t,
        ],
    );

    // Computed move uses the stored provider rate.
    run_ok(
        &home,
        &[
            "move",
            "10",
            "USD",
            "--from",
            "assets:usd",
            "--to",
            "assets:ves",
            "VES",
            "@bcv",
            "--effective-at",
            t,
        ],
    );

    let bal_personal = run_ok_out(&home, &["balance"]);
    assert!(bal_personal.contains("assets:ves\tVES\t452"));

    // Switch to another workspace: balances should be empty and rates should not exist.
    run_ok(&home, &["ws", "add", "Biz"]);
    run_ok(&home, &["ws", "checkout", "Biz"]);

    let bal_biz = run_ok_out(&home, &["balance"]);
    assert!(!bal_biz.contains("assets:ves\tVES\t"));
    assert!(!bal_biz.contains("assets:usd\tUSD\t"));

    // Move requiring provider rate should fail (no stored rates in this workspace).
    let mut cmd = bankero_cmd();
    cmd.env("BANKERO_HOME", home.path());
    cmd.args([
        "move",
        "10",
        "USD",
        "--from",
        "assets:usd",
        "--to",
        "assets:ves",
        "VES",
        "@bcv",
        "--effective-at",
        t,
    ]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("No stored rate for"));

    // Switching back should restore the original view.
    run_ok(&home, &["ws", "checkout", "personal"]);
    let bal_back = run_ok_out(&home, &["balance"]);
    assert!(bal_back.contains("assets:ves\tVES\t452"));
}

#[test]
fn ws_check_and_project_checkout_work_and_ws_checkout_resets_project() {
    let home = tempfile::tempdir().expect("tempdir");

    // Default context is deterministic.
    let out = run_ok_out(&home, &["ws", "check"]);
    assert!(out.contains("workspace: personal"));
    assert!(out.contains("current project is: 'default'"));

    run_ok(&home, &["ws", "add", "Startup-X"]);
    run_ok(&home, &["ws", "checkout", "Startup-X"]);

    run_ok(&home, &["project", "add", "Fix roof"]);
    run_ok(&home, &["project", "checkout", "Fix roof"]);

    let out = run_ok_out(&home, &["ws", "check"]);
    assert!(out.contains("workspace: Startup-X"));
    assert!(out.contains("current project is: 'Fix roof'"));

    // Switching workspaces resets the project back to default.
    run_ok(&home, &["ws", "checkout", "personal"]);
    let out = run_ok_out(&home, &["ws", "check"]);
    assert!(out.contains("workspace: personal"));
    assert!(out.contains("current project is: 'default'"));
}

#[test]
fn rate_set_get_list_roundtrip_is_deterministic() {
    let home = tempfile::tempdir().expect("tempdir");

    run_ok(
        &home,
        &[
            "rate",
            "set",
            "@bcv",
            "USD",
            "VES",
            "45.2",
            "--as-of",
            "2026-02-25T12:00:00Z",
        ],
    );
    run_ok(
        &home,
        &[
            "rate",
            "set",
            "@bcv",
            "USD",
            "VES",
            "46.0",
            "--as-of",
            "2026-02-26T12:00:00Z",
        ],
    );

    let out_get = run_ok_out(
        &home,
        &[
            "rate",
            "get",
            "@bcv",
            "USD",
            "VES",
            "--as-of",
            "2026-02-25T18:00:00Z",
        ],
    );
    assert!(out_get.contains("= 45.2"));

    let out_list = run_ok_out(
        &home,
        &["rate", "list", "@bcv", "USD", "VES", "--format", "tsv"],
    );
    assert!(out_list.contains("2026-02-25T12:00:00+00:00\t45.2"));
    assert!(out_list.contains("2026-02-26T12:00:00+00:00\t46.0"));

    let out_provider_only = run_ok_out(&home, &["rate", "list", "@bcv", "--format", "tsv"]);
    assert!(out_provider_only.contains("USD\tVES\t2026-02-26T12:00:00+00:00\t46.0"));
}

#[test]
fn sell_confirm_flow_writes_event_and_prints_value_preview() {
    let home = tempfile::tempdir().expect("tempdir");

    let mut cmd = bankero_cmd();
    cmd.env("BANKERO_HOME", home.path());
    cmd.args([
        "sell",
        "0.01",
        "BTC",
        "--from",
        "assets:btc",
        "--to",
        "assets:cash",
        "2400",
        "USD",
        "@binance",
        "--confirm",
        "--effective-at",
        "2026-02-25T12:00:00Z",
    ]);

    cmd.write_stdin("y\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("@binance rate is"))
        .stderr(predicate::str::contains("Transaction value:"));

    let out = run_ok_out(&home, &["balance"]);
    assert!(out.contains("assets:btc\tBTC\t-0.01"));
    assert!(out.contains("assets:cash\tUSD\t2400"));
}

#[test]
fn tag_fixed_basis_is_recorded_and_report_can_filter_by_tag() {
    let home = tempfile::tempdir().expect("tempdir");

    // Create a tagged event with a fixed basis.
    run_ok(
        &home,
        &[
            "tag",
            "assets:gold-bar",
            "--set-basis",
            "2000 USD",
            "--tag",
            "revalue",
            "--effective-at",
            "2026-02-25T12:00:00Z",
        ],
    );

    let all = run_ok_out(&home, &["report", "--month", "2026-02"]);
    assert!(all.contains("\ttag\t"));

    let filtered = run_ok_out(&home, &["report", "--month", "2026-02", "--tag", "revalue"]);
    assert!(filtered.contains("\ttag\t"));
}

#[test]
fn report_filters_by_range_account_and_commodity() {
    let home = tempfile::tempdir().expect("tempdir");

    // Two events in Feb, one in Mar.
    let feb1 = "2026-02-01T12:00:00Z";
    let feb2 = "2026-02-10T12:00:00Z";
    let mar1 = "2026-03-01T12:00:00Z";

    run_ok(
        &home,
        &[
            "deposit",
            "100",
            "USD",
            "--to",
            "assets:usd",
            "--from",
            "income:salary",
            "--effective-at",
            feb1,
        ],
    );

    run_ok(
        &home,
        &[
            "move",
            "10",
            "USD",
            "--from",
            "assets:usd",
            "--to",
            "assets:ves",
            "452",
            "VES",
            "@manual:45.2",
            "--effective-at",
            feb2,
        ],
    );

    run_ok(
        &home,
        &[
            "buy",
            "external:market",
            "5",
            "USD",
            "--from",
            "assets:usd",
            "--effective-at",
            mar1,
        ],
    );

    // Range filter should only keep Feb events.
    let out_range = run_ok_out(&home, &["report", "--range", "2026-02-01..2026-02-28"]);
    assert!(out_range.contains("\tdeposit\t"));
    assert!(out_range.contains("\tmove\t"));
    assert!(!out_range.contains("\tbuy\t"));

    // Account filter should keep only the move (it touches assets:ves).
    let out_account = run_ok_out(
        &home,
        &["report", "--month", "2026-02", "--account", "assets:ves"],
    );
    assert!(out_account.contains("\tmove\t"));
    assert!(!out_account.contains("\tdeposit\t"));

    // Commodity filter should keep only the move (it has a VES posting).
    let out_comm = run_ok_out(
        &home,
        &["report", "--month", "2026-02", "--commodity", "VES"],
    );
    assert!(out_comm.contains("\tmove\t"));
    assert!(!out_comm.contains("\tdeposit\t"));
}
