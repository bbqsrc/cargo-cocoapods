use std::path::Path;
use std::process::Command;

pub(crate) fn build(
    dir: &Path,
    triple: &str,
    cargo_args: &Vec<String>,
    is_nightly: bool,
) -> std::process::ExitStatus {
    let cargo_bin = "cargo";

    let mut cargo_cmd = Command::new(cargo_bin);

    if is_nightly {
        cargo_cmd.arg("+nightly");
    }

    cargo_cmd.arg("build");

    if is_nightly {
        cargo_cmd.args(&["-Z", "build-std"]);
    }

    cargo_cmd
        .args(cargo_args)
        .arg("--target")
        .arg(&triple)
        .current_dir(dir)
        .status()
        .expect("cargo crashed")
}
