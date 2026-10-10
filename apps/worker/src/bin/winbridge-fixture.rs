//! Explicit test executable; writes only into its chosen fixture working directory.
use std::{
    fs::OpenOptions,
    io::{self, Write},
    process::{Command, ExitCode},
    thread,
    time::Duration,
};
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("success");
    let mut counter = OpenOptions::new()
        .create(true)
        .append(true)
        .open("runs.log")
        .expect("fixture runs.log");
    writeln!(counter, "{} {mode}", std::process::id()).expect("fixture counter");
    println!("fixture pid={} mode={mode}", std::process::id());
    match mode {
        "success" => {
            println!("stdout: success Привет ✓");
            eprintln!("stderr: diagnostic");
            ExitCode::SUCCESS
        }
        "failure" => {
            eprintln!("stderr: deliberate failure");
            ExitCode::from(7)
        }
        "arguments" => {
            println!("{}", serde_json::to_string(&args[1..]).unwrap());
            ExitCode::SUCCESS
        }
        "flood" => {
            let bytes = [b'x'; 8192];
            for _ in 0..128 {
                io::stdout().write_all(&bytes).unwrap();
                io::stderr().write_all(&bytes).unwrap();
            }
            ExitCode::SUCCESS
        }
        "lines" => {
            for index in 0..20000 {
                println!("line {index:05} Привет ✓ 🚀");
                eprintln!("error {index:05} Привет ✓ 🚀");
            }
            ExitCode::SUCCESS
        }
        "tree" => {
            let mut child = Command::new(std::env::current_exe().unwrap())
                .arg("child")
                .spawn()
                .expect("fixture child");
            println!("child pid={}", child.id());
            io::stdout().flush().unwrap();
            let _ = child.wait();
            ExitCode::SUCCESS
        }
        "long" | "child" => loop {
            println!("heartbeat pid={}", std::process::id());
            io::stdout().flush().unwrap();
            thread::sleep(Duration::from_millis(100));
        },
        _ => {
            eprintln!("unknown fixture mode {mode}");
            ExitCode::from(2)
        }
    }
}
