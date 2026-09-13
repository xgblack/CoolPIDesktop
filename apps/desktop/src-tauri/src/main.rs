fn main() {
    if let Some(code) = host_core::terminal_handoff::run_if_requested() {
        std::process::exit(code);
    }
    cool_pi_desktop_lib::run();
}
