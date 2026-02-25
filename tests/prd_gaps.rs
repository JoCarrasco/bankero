//! These tests document gaps vs PRD/README examples.
//! They are ignored for now so we can freeze current behavior
//! while keeping a clear target for future fixes.

use assert_cmd::prelude::*;
use std::process::Command;

#[test]
fn prd_example_buy_without_payee_should_work() {
    let home = tempfile::tempdir().expect("tempdir");

    // PRD example: `bankero buy 500 USD --from assets:bank --to expenses:rent:450 --to expenses:water:50`
    // Current implementation requires a payee/target argument before amount.
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("bankero"));
    cmd.env("BANKERO_HOME", home.path());
    cmd.args([
        "buy",
        "500",
        "USD",
        "--from",
        "assets:bank",
        "--to",
        "expenses:rent:450",
        "--to",
        "expenses:water:50",
    ]);

    cmd.assert().success();
}
