use assert_cmd::Command;
use predicates::prelude::*;

fn bankero_cmd() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("bankero"))
}

#[test]
fn confirm_mode_prompts_for_rate_and_prints_value_preview() {
    let home = tempfile::tempdir().expect("tempdir");

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

    cmd.write_stdin("45.2\ny\n")
        .assert()
        .success()
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
fn confirm_mode_prompts_for_basis_amount_when_basis_provider_is_set() {
    let home = tempfile::tempdir().expect("tempdir");

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

    // First prompt: rate for @bcv (VES per USD), second prompt: basis amount (USD), then confirm.
    cmd.write_stdin("45.2\n100\ny\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("Enter basis amount"))
        .stderr(predicate::str::contains("Basis:"))
        .stderr(predicate::str::contains("Transaction value:"));
}
