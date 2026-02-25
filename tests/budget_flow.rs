use assert_cmd::prelude::*;
use std::process::Command;
use std::path::Path;

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

fn workspace_slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        let mapped = match ch {
            'a'..='z' | '0'..='9' | '-' | '_' => Some(ch),
            'A'..='Z' => Some(ch.to_ascii_lowercase()),
            ' ' | ':' | '/' | '\\' => Some('-'),
            _ => None,
        };
        if let Some(c) = mapped {
            if !(c == '-' && out.ends_with('-')) {
                out.push(c);
            }
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "workspace".to_string()
    } else {
        trimmed.to_string()
    }
}

fn workspace_db_path(bankero_home: &Path, workspace_name: &str) -> std::path::PathBuf {
    let slug = workspace_slug(workspace_name);
    bankero_home
        .join("data")
        .join("workspaces")
        .join(slug)
        .join("bankero.sqlite3")
}

#[test]
fn budget_create_and_report_shows_actual_spend_for_month() {
    let home = tempfile::tempdir().expect("tempdir");

    let t = "2026-02-25T12:00:00Z";

    run_ok(
        &home,
        &[
            "budget",
            "create",
            "Food",
            "300",
            "USD",
            "--month",
            "2026-02",
            "--category",
            "expenses:food",
        ],
    );

    run_ok(
        &home,
        &[
            "buy",
            "external:market",
            "50",
            "USD",
            "--from",
            "assets:bank",
            "--category",
            "expenses:food",
            "--effective-at",
            t,
        ],
    );

    let out = run_ok_out(&home, &["budget", "report", "--month", "2026-02"]);

    assert!(out.contains("month\tname\tcommodity\tbudget\tactual\tremaining"));
    assert!(out.contains("2026-02\tFood\tUSD\t300\t50\t250"));
}

#[test]
fn balance_shows_reserved_and_effective_for_account_scoped_budgets() {
    let home = tempfile::tempdir().expect("tempdir");

    let t = "2026-02-25T12:00:00Z";

    run_ok(
        &home,
        &[
            "deposit",
            "300",
            "USD",
            "--to",
            "assets:bank",
            "--from",
            "income:salary",
            "--effective-at",
            t,
        ],
    );

    run_ok(
        &home,
        &[
            "budget",
            "create",
            "Food",
            "300",
            "USD",
            "--month",
            "2026-02",
            "--category",
            "expenses:food",
            "--account",
            "assets:bank",
        ],
    );

    run_ok(
        &home,
        &[
            "buy",
            "external:market",
            "50",
            "USD",
            "--from",
            "assets:bank",
            "--category",
            "expenses:food",
            "--effective-at",
            t,
        ],
    );

    let out = run_ok_out(&home, &["balance", "assets:bank", "--month", "2026-02"]);
    assert!(out.contains("assets:bank\tUSD\t250"));
    assert!(out.contains("(reserved budgets)"));
    assert!(out.contains("assets:bank\tUSD\t-250"));
    assert!(out.contains("(effective balance)"));
    assert!(out.contains("assets:bank\tUSD\t0"));
}

#[test]
fn e2e_workspace_project_budget_income_and_spend_flow() {
    let home = tempfile::tempdir().expect("tempdir");

    let ws = "Startup-X";
    let project = "Fix roof";
    let t = "2026-02-25T12:00:00Z";

    run_ok(&home, &["ws", "add", ws]);
    run_ok(&home, &["ws", "checkout", ws]);

    run_ok(&home, &["project", "add", project]);
    run_ok(&home, &["project", "checkout", project]);

    run_ok(
        &home,
        &[
            "budget",
            "create",
            "Food",
            "300",
            "USD",
            "--month",
            "2026-02",
            "--category",
            "expenses:food",
            "--account",
            "assets:bank",
        ],
    );

    run_ok(
        &home,
        &[
            "deposit",
            "1000",
            "USD",
            "--to",
            "assets:bank",
            "--from",
            "income:salary",
            "--effective-at",
            t,
        ],
    );

    run_ok(
        &home,
        &[
            "buy",
            "external:market",
            "50",
            "USD",
            "--from",
            "assets:bank",
            "--category",
            "expenses:food",
            "--effective-at",
            t,
        ],
    );

    run_ok(
        &home,
        &[
            "buy",
            "external:market",
            "100",
            "USD",
            "--from",
            "assets:bank",
            "--category",
            "expenses:food",
            "--effective-at",
            t,
        ],
    );

    let report = run_ok_out(&home, &["budget", "report", "--month", "2026-02"]);
    assert!(report.contains("2026-02\tFood\tUSD\t300\t150\t150"));

    let bal = run_ok_out(&home, &["balance", "assets:bank", "--month", "2026-02"]);
    assert!(bal.contains("assets:bank\tUSD\t850"));
    assert!(bal.contains("(reserved budgets)"));
    assert!(bal.contains("assets:bank\tUSD\t-150"));
    assert!(bal.contains("(effective balance)"));
    assert!(bal.contains("assets:bank\tUSD\t700"));

    // Verify workspace/project crossover by inspecting the SQLite journal.
    let db_path = workspace_db_path(home.path(), ws);
    let conn = rusqlite::Connection::open(db_path).expect("open sqlite");
    let payload_json: String = conn
        .query_row(
            "SELECT payload_json FROM events ORDER BY created_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("read last payload");
    let v: serde_json::Value = serde_json::from_str(&payload_json).expect("payload json");
    assert_eq!(v.get("workspace").and_then(|x| x.as_str()), Some(ws));
    assert_eq!(v.get("project").and_then(|x| x.as_str()), Some(project));
}
