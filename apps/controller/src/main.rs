fn main() -> Result<(), Box<dyn std::error::Error>> {
    winbridge_core::run_scaffold_cli(winbridge_protocol::Component::Controller)
}
