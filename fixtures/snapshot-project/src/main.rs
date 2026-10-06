fn main() {
    println!("snapshot-fixture: {}", include_str!("../Привет.txt").trim());
    println!("cwd: {}", std::env::current_dir().unwrap().display());
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("serve") { return; }
    use std::{io::Write, time::Duration};
    let log = std::path::Path::new(args.get(1).expect("serve LOG_PATH [MODE]"));
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(log).unwrap();
    let mode = args.get(2).map(String::as_str).unwrap_or("ready");
    writeln!(file,"start pid={} · Привет ✓", std::process::id()).unwrap(); file.flush().unwrap();
    if mode == "crash" { eprintln!("fixture startup crash"); std::process::exit(7); }
    if mode != "silent" { println!("READY Привет ✓"); std::io::stdout().flush().unwrap(); }
    let mut tick = 0;
    loop {
        tick += 1;
        writeln!(file,"heartbeat {tick} · Привет ✓").unwrap(); file.flush().unwrap();
        if mode == "crash-late" && tick >= 10 { eprintln!("fixture runtime crash"); std::process::exit(9); }
        std::thread::sleep(Duration::from_millis(100));
    }
}
