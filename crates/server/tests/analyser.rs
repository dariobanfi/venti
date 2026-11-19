//! Tests of the analysis of an uploaded file in a child process, the way
//! the server runs it: with the real `venti-server` program.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use venti_flight::igc;
use venti_flight::testing::synthetic_flight;
use venti_server::flights::analyser::{Analyser, Failure, Where};
use venti_server::flights::derive::{Unusable, derive};

fn program() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_venti-server"))
}

fn child(time_allowed: Duration) -> Analyser {
    Analyser::with_time_allowed(Where::Child(program()), time_allowed)
}

fn sample() -> Arc<[u8]> {
    igc::write(&synthetic_flight()).into_bytes().into()
}

/// A flight file whose fixes jump from one side of the globe to the other,
/// which the analysis takes minutes to get through.
fn hostile(fixes: usize) -> Arc<[u8]> {
    let mut text = String::from("AXVE000\r\nHFDTEDATE:040726,01\r\n");
    for n in 0..fixes {
        let hemisphere = if n % 2 == 0 { 'N' } else { 'S' };
        let degrees = 79 - 79 * n / fixes;
        let east = 170 * n / fixes;
        text.push_str(&format!(
            "B{:02}{:02}{:02}{degrees:02}{:05}{hemisphere}{east:03}{:05}EA0100001000\r\n",
            10 + n / 3600,
            n / 60 % 60,
            n % 60,
            n % 60_000,
            (n * 7) % 60_000,
        ));
    }
    text.into_bytes().into()
}

#[tokio::test]
async fn the_child_gives_what_the_function_gives() {
    let analyser = child(Duration::from_secs(30));

    let derivation = analyser.analyse(sample()).await.unwrap();

    assert_eq!(derivation, derive(&sample()).unwrap());
    assert_eq!(derivation.identity, "6c55247b80a34b03fa0bb3bd38173c59");
}

#[tokio::test]
async fn a_file_that_is_no_flight_is_an_answer_not_a_failure() {
    let analyser = child(Duration::from_secs(30));
    for (file, reason) in [
        (&b""[..], Unusable::NoDate),
        (b"hello", Unusable::NoDate),
        (&[0xff, 0xfe, 0x00, 0x01], Unusable::NoDate),
        (b"AXVE000\r\nHFDTEDATE:040726,01\r\n", Unusable::NoFixes),
    ] {
        let result = analyser.analyse(file.to_vec().into()).await;
        assert_eq!(result, Err(Failure::Unusable(reason)));
    }
}

#[tokio::test]
async fn a_file_that_takes_too_long_costs_the_time_allowed_and_no_more() {
    let allowed = Duration::from_secs(1);
    let analyser = child(allowed);
    let began = Instant::now();

    let result = analyser.analyse(hostile(20_000)).await;

    let took = began.elapsed();
    assert_eq!(result, Err(Failure::TooSlow));
    assert!(took >= allowed, "{took:?}");
    assert!(took < allowed + Duration::from_secs(3), "{took:?}");

    // The child was killed, so its turn is free: three more of the same go
    // through one after another, each in the time allowed, where children
    // left running would make the third wait for a turn.
    let began = Instant::now();
    for n in 1..=3 {
        let result = analyser.analyse(hostile(20_000 + n)).await;
        assert_eq!(result, Err(Failure::TooSlow));
    }
    let took = began.elapsed();
    assert!(took < 3 * allowed + Duration::from_secs(6), "{took:?}");
    // And an honest file is analysed as if nothing had happened.
    let honest = child(Duration::from_secs(30)).analyse(sample()).await;
    assert!(honest.is_ok());

    // A killed child must also be collected, or a server that runs for
    // months fills the process table with the dead.
    #[cfg(target_os = "linux")]
    {
        let deadline = Instant::now() + Duration::from_secs(5);
        while dead_children() > 0 && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(dead_children(), 0, "killed children were left uncollected");
    }
}

/// How many children of this process have ended and were not collected.
#[cfg(target_os = "linux")]
fn dead_children() -> usize {
    let me = std::process::id().to_string();
    let Ok(processes) = std::fs::read_dir("/proc") else {
        return 0;
    };
    processes
        .filter_map(|entry| std::fs::read_to_string(entry.ok()?.path().join("stat")).ok())
        .filter(|stat| {
            // "pid (name) state ppid ...": the name may hold spaces, so
            // read from its closing bracket.
            let Some((_, rest)) = stat.rsplit_once(") ") else {
                return false;
            };
            let mut fields = rest.split(' ');
            fields.next() == Some("Z") && fields.next() == Some(me.as_str())
        })
        .count()
}

#[tokio::test]
async fn the_child_is_given_nothing_of_the_servers_environment() {
    // The derive subcommand reads its input and nothing else. Run by hand
    // with a database password in the environment, as the server has one,
    // it must work without it and never repeat it.
    let mut child = Command::new(program())
        .arg("derive")
        .env("VENTI_DATABASE_URL", "postgres://venti:hunter2@db/venti")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(&sample()).unwrap();
    }
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let answer: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        answer["flight"]["identity"],
        "6c55247b80a34b03fa0bb3bd38173c59"
    );
    assert!(output.stderr.is_empty());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("hunter2"));

    // It takes no arguments: a file name is not read.
    let refused = Command::new(program())
        .args(["derive", "/etc/passwd"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(refused.stdout.is_empty());
}
