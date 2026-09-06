// The build script of the sealed-environment control (`generate::verify::tests`): it prints
// every proxy variable it can see and fails, so its output lands in the verification error. It
// lives beside the test harness rather than in `src/` because the presentation scan forbids a
// print macro in shipped source — and this file is a fixture, not shipped code.
fn main() {
    let mut seen: Vec<String> = std::env::vars()
        .filter(|(name, _)| name.to_ascii_lowercase().contains("proxy"))
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    seen.sort();
    eprintln!(
        "proxy variables seen by the build script: {}",
        seen.join(" ")
    );
    std::process::exit(1);
}
