fn main() {
    println!("snapshot-fixture: {}", include_str!("../Привет.txt").trim());
    println!("cwd: {}", std::env::current_dir().unwrap().display());
}
