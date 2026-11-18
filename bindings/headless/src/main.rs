//! `kition-doc`: serves [`kition_doc::backend`] when Kition launches the kit.

fn main() -> std::process::ExitCode {
    match kition_doc::kit() {
        Ok(kit) => kition_headless::serve_backend(&kit, kition_doc::backend(&kit)),
        Err(error) => {
            eprintln!("kit: {error}");
            std::process::ExitCode::from(2)
        }
    }
}
