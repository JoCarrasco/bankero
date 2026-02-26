use assert_cmd::prelude::*;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

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

fn run_fail_out(home: &tempfile::TempDir, args: &[&str]) -> String {
    let mut cmd = bankero_cmd();
    cmd.env("BANKERO_HOME", home.path());
    cmd.args(args);
    let out = cmd.assert().failure().get_output().clone();
    let mut combined = Vec::new();
    combined.extend_from_slice(&out.stdout);
    combined.extend_from_slice(&out.stderr);
    String::from_utf8(combined).expect("utf8 output")
}

fn spawn_expose(home: &tempfile::TempDir) -> (Child, mpsc::Receiver<String>) {
    let mut cmd = bankero_cmd();
    cmd.env("BANKERO_HOME", home.path());
    cmd.args([
        "sync",
        "expose",
        "--test-bind",
        "127.0.0.1",
        "--test-udp-port",
        "0",
        "--test-tcp-port",
        "0",
        "--test-once",
        "--test-print-ports",
    ]);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("spawn expose");
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    let (tx, rx) = mpsc::channel::<String>();
    let tx_err = tx.clone();

    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });

    // Drain stderr so the child can't block if it writes.
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            let _ = tx_err.send(format!("[stderr] {line}"));
        }
    });

    (child, rx)
}

fn spawn_expose_interactive(
    home: &tempfile::TempDir,
) -> (Child, std::process::ChildStdin, mpsc::Receiver<String>) {
    let mut cmd = bankero_cmd();
    cmd.env("BANKERO_HOME", home.path());
    cmd.args([
        "sync",
        "expose",
        "--test-bind",
        "127.0.0.1",
        "--test-udp-port",
        "0",
        "--test-tcp-port",
        "0",
        "--test-print-ports",
    ]);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("spawn expose");
    let stdin = child.stdin.take().expect("stdin piped");
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    let (tx, rx) = mpsc::channel::<String>();
    let tx_err = tx.clone();

    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });

    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            let _ = tx_err.send(format!("[stderr] {line}"));
        }
    });

    (child, stdin, rx)
}

fn wait_for_lan_udp(rx: &mpsc::Receiver<String>) -> String {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(remaining.min(Duration::from_millis(200))) {
            Ok(line) => {
                if let Some(rest) = line.strip_prefix("lan_udp\t") {
                    return rest.trim().to_string();
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(err) => panic!("expose output channel closed: {err}"),
        }
    }
    panic!("Timed out waiting for expose to print lan_udp")
}

#[test]
fn lan_sync_isolated_and_deterministic_in_ci() {
    let home_a = tempfile::tempdir().expect("tempdir home_a");
    let home_b = tempfile::tempdir().expect("tempdir home_b");

    println!("[lan_sync_ci] starting (two isolated BANKERO_HOME dirs)");

    // Give both devices a friendly name (not required, but makes logs easier).
    run_ok(&home_a, &["login", "--name", "juicy_strawberry"]);
    run_ok(&home_b, &["login", "--name", "zesty_kiwi"]);

    // Seed an event on A.
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

    println!("[lan_sync_ci] exposing device A on localhost (ephemeral ports)");

    // Start A's expose server on localhost with ephemeral ports.
    let (mut child, rx) = spawn_expose(&home_a);
    let lan_udp = wait_for_lan_udp(&rx);

    println!("[lan_sync_ci] discovering via --target {lan_udp}");

    // Discover using the printed UDP address (no broadcast; deterministic).
    let out = run_ok_out(
        &home_b,
        &[
            "sync",
            "discover",
            "--target",
            &lan_udp,
            "--timeout-ms",
            "800",
        ],
    );
    assert!(out.contains("@1"), "discover output: {out}");

    // Sync from B to A and back.
    println!("[lan_sync_ci] syncing via handle @1 all");
    run_ok(&home_b, &["sync", "@1", "all"]);

    println!("[lan_sync_ci] verifying balance on device B");
    let out = run_ok_out(&home_b, &["balance", "assets:cash"]);
    assert!(
        out.contains("assets:cash\tUSD\t100"),
        "balance output: {out}"
    );

    // Expose should exit after one sync.
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            assert!(status.success(), "expose exited with {status}");
            break;
        }
        if start.elapsed() > Duration::from_secs(3) {
            let _ = child.kill();
            panic!("expose did not exit in time");
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    println!("[lan_sync_ci] complete");
}

#[test]
fn lan_sync_expose_prompts_and_keeps_listening() {
    let home_a = tempfile::tempdir().expect("tempdir home_a");
    let home_b = tempfile::tempdir().expect("tempdir home_b");

    println!("[lan_sync_ci] starting interactive accept/reject test");

    run_ok(&home_a, &["login", "--name", "prompty_mango"]);
    run_ok(&home_b, &["login", "--name", "clienty_grape"]);

    run_ok(
        &home_a,
        &[
            "deposit",
            "42",
            "USD",
            "--to",
            "assets:cash",
            "--from",
            "income:salary",
            "--effective-at",
            "2026-02-25T12:00:00Z",
        ],
    );

    let (mut child, mut stdin, rx) = spawn_expose_interactive(&home_a);
    let lan_udp = wait_for_lan_udp(&rx);

    // Feed answers for two incoming connections: first reject, then accept.
    // The expose server should keep listening after the first rejection.
    use std::io::Write as _;
    stdin.write_all(b"n\n").expect("write reject answer");
    stdin.write_all(b"y\n").expect("write accept answer");

    let out = run_ok_out(
        &home_b,
        &[
            "sync",
            "discover",
            "--target",
            &lan_udp,
            "--timeout-ms",
            "800",
        ],
    );
    assert!(out.contains("@1"), "discover output: {out}");

    // First attempt should be rejected.
    let out = run_fail_out(&home_b, &["sync", "@1", "all"]);
    assert!(
        out.contains("Sync rejected") || out.contains("rejected"),
        "sync output: {out}"
    );

    // Second attempt should succeed.
    run_ok(&home_b, &["sync", "@1", "all"]);

    let out = run_ok_out(&home_b, &["balance", "assets:cash"]);
    assert!(
        out.contains("assets:cash\tUSD\t42"),
        "balance output: {out}"
    );

    let _ = child.kill();
    let _ = child.wait();
    println!("[lan_sync_ci] interactive prompt test complete");
}
