use assert_cmd::Command;
use predicates::prelude::*;

fn bankero_cmd() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("bankero"))
}

#[test]
fn confirm_mode_uses_stored_rate_and_prints_value_preview() {
    let home = tempfile::tempdir().expect("tempdir");

    // Store a provider rate offline (VES per USD).
    let mut rate = bankero_cmd();
    rate.env("BANKERO_HOME", home.path());
    rate.args([
        "rate",
        "set",
        "@binance",
        "USD",
        "VES",
        "45.2",
        "--as-of",
        "2026-02-25T12:00:00Z",
    ]);
    rate.assert().success();

    // PRD example style:
    // bankero move 5000 VES --from assets:wallet --to external:neighbor @binance --confirm
    // > Binance rate is 45.2. Transaction value: 110.61 USD. Proceed? [Y/n]

    let mut cmd = bankero_cmd();
    cmd.env("BANKERO_HOME", home.path());
    cmd.args([
        "move",
        "5000",
        "VES",
        "--from",
        "assets:wallet",
        "--to",
        "external:neighbor",
        "@binance",
        "--confirm",
        "--effective-at",
        "2026-02-25T12:00:00Z",
    ]);

    cmd.write_stdin("y\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("Using @binance rate"))
        .stderr(predicate::str::contains("Transaction value:"));

    // Ensure the event was committed (balance should show the movement).
    let mut bal = bankero_cmd();
    bal.env("BANKERO_HOME", home.path());
    bal.args(["balance"]);
    bal.assert()
        .success()
        .stdout(predicate::str::contains("assets:wallet\tVES\t-5000"));
}

#[test]
fn confirm_mode_errors_if_provider_rate_missing() {
    let home = tempfile::tempdir().expect("tempdir");

    let mut cmd = bankero_cmd();
    cmd.env("BANKERO_HOME", home.path());
    cmd.args([
        "move",
        "5000",
        "VES",
        "--from",
        "assets:wallet",
        "--to",
        "external:neighbor",
        "@binance",
        "--confirm",
        "--effective-at",
        "2026-02-25T12:00:00Z",
    ]);

    cmd.write_stdin("y\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("No stored rate for @binance"));
}

#[test]
fn confirm_mode_computes_basis_deterministically_when_basis_provider_is_set() {
    let home = tempfile::tempdir().expect("tempdir");

    // Store a provider rate offline (VES per USD) for @bcv.
    let mut rate = bankero_cmd();
    rate.env("BANKERO_HOME", home.path());
    rate.args([
        "rate",
        "set",
        "@bcv",
        "USD",
        "VES",
        "45.2",
        "--as-of",
        "2026-02-25T12:00:00Z",
    ]);
    rate.assert().success();

    // Store a basis provider rate offline as well.
    // We interpret stored rates as: <quote> per <base>.
    // So: USD -> VES at 50 means 1 USD = 50 VES; basis in USD for 840 VES is 840/50 = 16.8 USD.
    let mut basis_rate = bankero_cmd();
    basis_rate.env("BANKERO_HOME", home.path());
    basis_rate.args([
        "rate",
        "set",
        "@binance",
        "USD",
        "VES",
        "50",
        "--as-of",
        "2026-02-25T12:00:00Z",
    ]);
    basis_rate.assert().success();

    // PRD example style:
    // bankero buy external:farmatodo 840 VES --from assets:mercantil @bcv -b @binance --confirm

    let mut cmd = bankero_cmd();
    cmd.env("BANKERO_HOME", home.path());
    cmd.args([
        "buy",
        "external:farmatodo",
        "840",
        "VES",
        "--from",
        "assets:mercantil",
        "@bcv",
        "-b",
        "@binance",
        "--confirm",
        "--effective-at",
        "2026-02-25T12:00:00Z",
    ]);

    cmd.write_stdin("y\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("Basis:"))
        .stderr(predicate::str::contains("Transaction value:"));
}
