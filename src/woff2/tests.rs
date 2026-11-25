use super::*;
use std::{fs, process::Command};

fn shell(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command.arg("-c").arg(script);
    command
}

#[test]
fn woff2_worker_drains_output_while_sending_a_large_input() {
    // The child fills stdout before it reads stdin. A single-threaded parent
    // would deadlock once both pipes fill.
    let input = vec![b'x'; 1024 * 1024];
    let command = shell("head -c 524288 /dev/zero; cat >/dev/null");
    let output = run(
        command,
        &input,
        600_000,
        Instant::now() + Duration::from_secs(4),
    )
    .unwrap();
    assert_eq!(output.len(), 524_288);
    assert!(output.iter().all(|byte| *byte == 0));
}

#[test]
fn woff2_worker_rejects_overlong_output_and_unsuccessful_exit() {
    let deadline = || Instant::now() + Duration::from_secs(3);
    assert!(
        run(
            shell("head -c 10000 /dev/zero; cat >/dev/null"),
            b"font",
            1024,
            deadline()
        )
        .is_err()
    );
    assert!(run(shell("cat >/dev/null; exit 3"), b"font", 1024, deadline()).is_err());
    assert!(run(shell("exit 0"), &vec![0; MAX_FONT], 1024, deadline()).is_err());
    assert!(
        run(
            Command::new("/path/that/does/not/exist/reader-woff2-worker"),
            b"font",
            1024,
            deadline()
        )
        .is_err()
    );
}

#[test]
fn woff2_worker_timeout_kills_and_reaps_the_child() {
    let path = std::env::temp_dir().join(format!(
        "reader-woff2-worker-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let _ = fs::remove_file(&path);
    let mut command = shell("echo $$ > \"$READER_TEST_PID\"; exec sleep 30");
    command.env("READER_TEST_PID", &path);
    let start = Instant::now();
    assert!(run(command, b"font", 1024, start + Duration::from_millis(250)).is_err());
    assert!(start.elapsed() < Duration::from_secs(3));
    let text = fs::read_to_string(&path).unwrap();
    fs::remove_file(&path).unwrap();
    let pid: libc::pid_t = text.trim().parse().unwrap();
    // SAFETY: signal 0 checks whether this exact process exists without sending a signal.
    let alive = unsafe { libc::kill(pid, 0) };
    assert_eq!(alive, -1, "the timed-out helper must be reaped");
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
}

#[test]
#[ignore = "Requires an explicitly built debug reader-woff2-worker at READER_WOFF2_TEST_HELPER"]
fn woff2_pinned_fixture_decodes_in_the_isolated_worker() {
    use std::io::Read;
    let file = fs::File::open(crate::test_fixture("publisher-woff2.epub")).unwrap();
    let mut zip = zip::ZipArchive::new(file).unwrap();
    let mut input = Vec::new();
    zip.by_name("EPUB/fonts/Silkscreen-Regular.woff2")
        .unwrap()
        .read_to_end(&mut input)
        .unwrap();
    let output = decode(&input, MAX_FONT, Instant::now() + Duration::from_secs(4)).unwrap();
    assert!(skrifa::FontRef::new(&output).is_ok());
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "Requires a Linux reader-woff2-worker at READER_WOFF2_TEST_HELPER"]
fn woff2_linux_worker_installs_limits_before_reading_input() {
    let path = std::env::var_os("READER_WOFF2_TEST_HELPER").unwrap();
    let mut command = Command::new(path);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let child = command.spawn().unwrap();
    let mut guard = Guard(child, false);
    let pid = guard.0.id();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let limits = fs::read_to_string(format!("/proc/{pid}/limits")).unwrap();
        let address = limits
            .lines()
            .find(|line| line.starts_with("Max address space"))
            .unwrap();
        let core = limits
            .lines()
            .find(|line| line.starts_with("Max core file size"))
            .unwrap();
        let cpu = limits
            .lines()
            .find(|line| line.starts_with("Max cpu time"))
            .unwrap();
        if address.split_whitespace().any(|value| value == "67108864")
            && core
                .split_whitespace()
                .filter(|value| *value == "0")
                .count()
                >= 2
            && cpu.split_whitespace().any(|value| value == "2")
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "worker did not install its limits"
        );
        assert!(
            guard.0.try_wait().unwrap().is_none(),
            "worker exited before reading input"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    // Dropping the guard kills and reaps the blocked helper.
}
