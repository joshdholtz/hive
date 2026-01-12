use std::io::{Read, Write};
use std::time::Duration;

/// Test that carriage return (\r) causes input to be processed in a PTY
/// This simulates what happens when we send a nudge to Claude/Codex
#[test]
fn test_pty_enter_with_cat() {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("Failed to open PTY");

    // Spawn cat which echoes stdin to stdout
    let cmd = CommandBuilder::new("cat");
    let _child = pair.slave.spawn_command(cmd).expect("Failed to spawn cat");

    let mut writer = pair.master.take_writer().expect("Failed to take writer");
    let mut reader = pair.master.try_clone_reader().expect("Failed to clone reader");

    // Send "hello" followed by Enter (\r)
    writer.write_all(b"hello\r").expect("Failed to write");
    writer.flush().expect("Failed to flush");

    // Give cat time to process
    std::thread::sleep(Duration::from_millis(100));

    // Send EOF to cat (Ctrl-D)
    writer.write_all(b"\x04").expect("Failed to send EOF");
    writer.flush().expect("Failed to flush EOF");

    // Read output
    let mut output = Vec::new();
    let _ = reader.read_to_end(&mut output);

    let output_str = String::from_utf8_lossy(&output);
    println!("PTY output: {:?}", output_str);

    // cat should have echoed "hello" back
    assert!(output_str.contains("hello"), "Expected 'hello' in output, got: {}", output_str);
}

/// Test that newline (\n) also works for PTY input
#[test]
fn test_pty_newline_with_cat() {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("Failed to open PTY");

    let cmd = CommandBuilder::new("cat");
    let _child = pair.slave.spawn_command(cmd).expect("Failed to spawn cat");

    let mut writer = pair.master.take_writer().expect("Failed to take writer");
    let mut reader = pair.master.try_clone_reader().expect("Failed to clone reader");

    // Send "world" followed by newline (\n)
    writer.write_all(b"world\n").expect("Failed to write");
    writer.flush().expect("Failed to flush");

    std::thread::sleep(Duration::from_millis(100));

    writer.write_all(b"\x04").expect("Failed to send EOF");
    writer.flush().expect("Failed to flush EOF");

    let mut output = Vec::new();
    let _ = reader.read_to_end(&mut output);

    let output_str = String::from_utf8_lossy(&output);
    println!("PTY output with \\n: {:?}", output_str);

    assert!(output_str.contains("world"), "Expected 'world' in output, got: {}", output_str);
}

/// Test escape + message + enter sequence
#[test]
fn test_pty_escape_message_enter() {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("Failed to open PTY");

    let cmd = CommandBuilder::new("cat");
    let _child = pair.slave.spawn_command(cmd).expect("Failed to spawn cat");

    let mut writer = pair.master.take_writer().expect("Failed to take writer");
    let mut reader = pair.master.try_clone_reader().expect("Failed to clone reader");

    // Send Escape + message + Enter (what nudge does)
    let message = "test message";
    let bytes = format!("\x1b{}\r", message);
    writer.write_all(bytes.as_bytes()).expect("Failed to write");
    writer.flush().expect("Failed to flush");

    std::thread::sleep(Duration::from_millis(100));

    writer.write_all(b"\x04").expect("Failed to send EOF");
    writer.flush().expect("Failed to flush EOF");

    let mut output = Vec::new();
    let _ = reader.read_to_end(&mut output);

    let output_str = String::from_utf8_lossy(&output);
    println!("PTY output with ESC+msg+CR: {:?}", output_str);

    // The message should appear in output (cat echoes everything)
    assert!(output_str.contains("test message"), "Expected 'test message' in output, got: {}", output_str);
}

/// Test with a shell script that simulates readline behavior
#[test]
fn test_pty_with_read_command() {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("Failed to open PTY");

    // Use bash with read command which waits for Enter
    let mut cmd = CommandBuilder::new("bash");
    cmd.args(["-c", "read line && echo \"GOT: $line\""]);

    let _child = pair.slave.spawn_command(cmd).expect("Failed to spawn bash");

    let mut writer = pair.master.take_writer().expect("Failed to take writer");
    let mut reader = pair.master.try_clone_reader().expect("Failed to clone reader");

    // Send input + Enter
    writer.write_all(b"myinput\r").expect("Failed to write");
    writer.flush().expect("Failed to flush");

    // Wait for bash to process
    std::thread::sleep(Duration::from_millis(200));

    // Read output non-blocking
    let mut output = Vec::new();

    // Set non-blocking read with timeout
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
    });

    let _ = reader.read_to_end(&mut output);

    let output_str = String::from_utf8_lossy(&output);
    println!("Bash read output: {:?}", output_str);

    // Should see "GOT: myinput" in output
    assert!(output_str.contains("GOT: myinput"), "Expected 'GOT: myinput' in output, got: {}", output_str);
}

/// Test different key sequences to find what works
#[test]
fn test_various_enter_sequences() {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};

    let sequences = vec![
        ("CR only", b"\r".to_vec()),
        ("LF only", b"\n".to_vec()),
        ("CRLF", b"\r\n".to_vec()),
        ("ESC + CR", b"\x1b\r".to_vec()),
    ];

    for (name, enter_seq) in sequences {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("Failed to open PTY");

        let mut cmd = CommandBuilder::new("bash");
        cmd.args(["-c", "read line && echo \"RECEIVED: $line\""]);

        let _child = pair.slave.spawn_command(cmd).expect("Failed to spawn");

        let mut writer = pair.master.take_writer().expect("Failed to take writer");
        let mut reader = pair.master.try_clone_reader().expect("Failed to clone reader");

        // Send "test" + enter sequence
        writer.write_all(b"test").expect("Failed to write");
        writer.write_all(&enter_seq).expect("Failed to write enter");
        writer.flush().expect("Failed to flush");

        std::thread::sleep(Duration::from_millis(200));

        let mut output = Vec::new();
        let _ = reader.read_to_end(&mut output);

        let output_str = String::from_utf8_lossy(&output);
        let success = output_str.contains("RECEIVED: test");
        println!("{}: {} - output: {:?}", name, if success { "PASS" } else { "FAIL" }, output_str);
    }
}

/// Test with actual Codex CLI if available
/// This test is ignored by default - run with: cargo test test_codex_cli -- --ignored --nocapture
#[test]
#[ignore]
fn test_codex_cli_input() {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};
    use std::process::Command;

    // Check if codex is available
    let codex_check = Command::new("which").arg("codex").output();
    if codex_check.is_err() || !codex_check.unwrap().status.success() {
        println!("Codex CLI not found, skipping test");
        return;
    }

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 40,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("Failed to open PTY");

    // Start codex with a simple prompt
    let mut cmd = CommandBuilder::new("codex");
    cmd.args(["--sandbox", "danger-full-access", "--ask-for-approval", "never", "echo hello"]);
    cmd.env("TERM", "xterm-256color");

    println!("Starting Codex CLI...");
    let _child = pair.slave.spawn_command(cmd).expect("Failed to spawn codex");

    let mut writer = pair.master.take_writer().expect("Failed to take writer");
    let mut reader = pair.master.try_clone_reader().expect("Failed to clone reader");

    // Wait for Codex to start and process initial prompt
    println!("Waiting for Codex to initialize...");
    std::thread::sleep(Duration::from_secs(5));

    // Read initial output
    let mut initial_output = vec![0u8; 8192];
    let n = reader.read(&mut initial_output).unwrap_or(0);
    println!("Initial output ({} bytes): {:?}", n, String::from_utf8_lossy(&initial_output[..n]));

    // Now try to send a follow-up prompt
    println!("Sending follow-up prompt...");
    let nudge_message = "echo 'nudge received'";
    let bytes = format!("{}\r", nudge_message);
    writer.write_all(bytes.as_bytes()).expect("Failed to write nudge");
    writer.flush().expect("Failed to flush");

    // Wait for processing
    std::thread::sleep(Duration::from_secs(3));

    // Read output after nudge
    let mut nudge_output = vec![0u8; 8192];
    let n = reader.read(&mut nudge_output).unwrap_or(0);
    println!("Nudge output ({} bytes): {:?}", n, String::from_utf8_lossy(&nudge_output[..n]));

    // Send Ctrl-C to exit
    writer.write_all(b"\x03").expect("Failed to send Ctrl-C");
    writer.flush().expect("Failed to flush");

    println!("Test complete - check output above to see if nudge was processed");
}

/// Test with ink-like raw mode TUI (Node.js script that mimics ink's input handling)
/// This test helps understand how TUI apps in raw mode receive input
/// Run with: cargo test test_ink_style_input -- --ignored --nocapture
#[test]
#[ignore]
fn test_ink_style_input() {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};
    use std::process::Command;

    // Check if node is available
    let node_check = Command::new("which").arg("node").output();
    if node_check.is_err() || !node_check.unwrap().status.success() {
        println!("Node not found, skipping test");
        return;
    }

    let test_script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("ink_input_test.js");

    if !test_script.exists() {
        println!("Test script not found: {:?}", test_script);
        return;
    }

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("Failed to open PTY");

    let mut cmd = CommandBuilder::new("node");
    cmd.arg(&test_script);
    cmd.env("TERM", "xterm-256color");

    println!("Starting ink-style input test...");
    let _child = pair.slave.spawn_command(cmd).expect("Failed to spawn node");

    let mut writer = pair.master.take_writer().expect("Failed to take writer");
    let mut reader = pair.master.try_clone_reader().expect("Failed to clone reader");

    // Wait for script to initialize
    std::thread::sleep(Duration::from_millis(500));

    // Read initial output
    let mut initial_output = vec![0u8; 4096];
    let n = reader.read(&mut initial_output).unwrap_or(0);
    println!("Initial output ({} bytes): {:?}", n, String::from_utf8_lossy(&initial_output[..n]));

    // Test 1: Send text + CR in one write
    println!("\n--- Test 1: text + CR together ---");
    writer.write_all(b"hello\r").expect("Failed to write");
    writer.flush().expect("Failed to flush");
    std::thread::sleep(Duration::from_millis(200));

    let mut output1 = vec![0u8; 4096];
    let n = reader.read(&mut output1).unwrap_or(0);
    println!("Output after 'hello\\r': {:?}", String::from_utf8_lossy(&output1[..n]));

    // Test 2: Send text, delay, then CR separately
    println!("\n--- Test 2: text then CR separately ---");
    writer.write_all(b"world").expect("Failed to write");
    writer.flush().expect("Failed to flush");
    std::thread::sleep(Duration::from_millis(100));
    writer.write_all(b"\r").expect("Failed to write CR");
    writer.flush().expect("Failed to flush CR");
    std::thread::sleep(Duration::from_millis(200));

    let mut output2 = vec![0u8; 4096];
    let n = reader.read(&mut output2).unwrap_or(0);
    println!("Output after 'world' then CR: {:?}", String::from_utf8_lossy(&output2[..n]));

    // Test 3: Send text + LF instead of CR
    println!("\n--- Test 3: text + LF ---");
    writer.write_all(b"test\n").expect("Failed to write");
    writer.flush().expect("Failed to flush");
    std::thread::sleep(Duration::from_millis(200));

    let mut output3 = vec![0u8; 4096];
    let n = reader.read(&mut output3).unwrap_or(0);
    println!("Output after 'test\\n': {:?}", String::from_utf8_lossy(&output3[..n]));

    // Send Ctrl+C to exit
    writer.write_all(b"\x03").expect("Failed to send Ctrl+C");
    writer.flush().expect("Failed to flush");

    println!("\nTest complete");
}

/// Test with actual Claude CLI if available
/// This test is ignored by default - run with: cargo test test_claude_cli -- --ignored --nocapture
#[test]
#[ignore]
fn test_claude_cli_input() {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};
    use std::process::Command;

    // Check if claude is available
    let claude_check = Command::new("which").arg("claude").output();
    if claude_check.is_err() || !claude_check.unwrap().status.success() {
        println!("Claude CLI not found, skipping test");
        return;
    }

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("Failed to open PTY");

    // Start claude with a simple prompt
    let mut cmd = CommandBuilder::new("claude");
    cmd.arg("echo hello");
    cmd.env("TERM", "xterm-256color");

    println!("Starting Claude CLI...");
    let _child = pair.slave.spawn_command(cmd).expect("Failed to spawn claude");

    let mut writer = pair.master.take_writer().expect("Failed to take writer");
    let mut reader = pair.master.try_clone_reader().expect("Failed to clone reader");

    // Wait for Claude to start and process initial prompt
    println!("Waiting for Claude to initialize...");
    std::thread::sleep(Duration::from_secs(5));

    // Read initial output
    let mut initial_output = vec![0u8; 8192];
    let n = reader.read(&mut initial_output).unwrap_or(0);
    println!("Initial output ({} bytes): {:?}", n, String::from_utf8_lossy(&initial_output[..n]));

    // Now try to send a follow-up prompt
    println!("Sending follow-up prompt...");
    let nudge_message = "echo 'nudge received'";
    let bytes = format!("{}\r", nudge_message);
    writer.write_all(bytes.as_bytes()).expect("Failed to write nudge");
    writer.flush().expect("Failed to flush");

    // Wait for processing
    std::thread::sleep(Duration::from_secs(3));

    // Read output after nudge
    let mut nudge_output = vec![0u8; 8192];
    let n = reader.read(&mut nudge_output).unwrap_or(0);
    println!("Nudge output ({} bytes): {:?}", n, String::from_utf8_lossy(&nudge_output[..n]));

    // Send Ctrl-C to exit
    writer.write_all(b"\x03").expect("Failed to send Ctrl-C");
    writer.flush().expect("Failed to flush");

    println!("Test complete - check output above to see if nudge was processed");
}
