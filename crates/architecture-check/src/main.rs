use std::process::Command;

fn main() {
    let command = std::env::args().nth(1).unwrap_or_default();
    if command != "check" {
        eprintln!("Usage: architecture-check check");
        std::process::exit(2);
    }
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .output()
        .unwrap_or_else(|error| {
            eprintln!("failed to execute cargo metadata: {error}");
            std::process::exit(1);
        });
    if !output.status.success() {
        eprintln!("cargo metadata failed");
        std::process::exit(1);
    }
    let metadata: architecture_check::Metadata = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| {
            eprintln!("invalid cargo metadata output: {error}");
            std::process::exit(1);
        });
    if let Err(violations) = architecture_check::validate(&metadata) {
        for violation in violations {
            eprintln!("Architecture violation: {violation}");
        }
        std::process::exit(1);
    }
    println!("Cargo metadata architecture fitness: PASS");
}
