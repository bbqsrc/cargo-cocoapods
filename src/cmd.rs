use std::{
    ffi::OsStr,
    io,
    path::{Path, PathBuf},
    process::Output,
};

pub fn lipo<S>(items: impl Iterator<Item = S>, output_path: &Path) -> io::Result<Output>
where
    S: AsRef<OsStr>,
{
    let mut cmd = std::process::Command::new("lipo");
    cmd.arg("-create").arg("-output").arg(output_path);
    items.for_each(|item| {
        cmd.arg(item);
    });
    cmd.output()
}

pub struct Xcodebuild;

impl Xcodebuild {
    pub fn create_xcframework_frameworks<P: AsRef<Path>>(
        name: &str,
        paths: impl Iterator<Item = P>,
        output_path: &Path,
    ) -> io::Result<Output> {
        let mut cmd = std::process::Command::new("xcodebuild");
        cmd.arg("-create-xcframework")
            .arg("-output")
            .arg(output_path.join(format!("{name}.xcframework")));
        paths.for_each(|path| {
            cmd.arg("-framework").arg(path.as_ref());
        });
        cmd.output()
    }
}

pub struct Swiftc;

impl Swiftc {
    pub fn build(
        triple: &str,
        min_versions: &MinVersions,
        module_name: &str,
        frameworks_path: &Path,
        swift_files: &[PathBuf],
    ) -> String {
        let sdk = current_sdk(triple);
        let swift_triple = current_triple(triple, min_versions);
        let obj_name = format!("{}.o", module_name);

        let mut output = std::process::Command::new("swiftc")
            .args([
                "-emit-library",
                "-emit-object",
                "-static",
                "-sdk",
                &sdk,
                "-target",
                &swift_triple,
                "-module-name",
                module_name,
                "-o",
                &obj_name,
                "-F",
            ])
            .arg(frameworks_path)
            .args(swift_files)
            .spawn()
            .unwrap();
        output.wait().unwrap();

        let mut output = std::process::Command::new("swiftc")
            .args([
                "-emit-module",
                "-static",
                "-sdk",
                &sdk,
                "-enable-library-evolution",
                "-emit-parseable-module-interface",
                "-target",
                &swift_triple,
                "-module-name",
                module_name,
                "-F",
            ])
            .arg(frameworks_path)
            .args(swift_files)
            .spawn()
            .unwrap();
        output.wait().unwrap();

        obj_name
    }
}

pub struct Ar;

impl Ar {
    pub fn insert(path: &Path, input: &str) {
        let _output = std::process::Command::new("ar")
            .arg("q")
            .arg(path)
            .arg(input)
            .output()
            .unwrap();
    }
}

fn current_sdk(triple: &str) -> String {
    let output = std::process::Command::new("xcrun")
        .args(["--show-sdk-path", "--sdk"])
        .arg(match triple {
            "aarch64-apple-darwin" => "macosx",
            "aarch64-apple-ios" => "iphoneos",
            "aarch64-apple-ios-sim" => "iphonesimulator",
            "x86_64-apple-darwin" => "macosx",
            "x86_64-apple-ios" => "iphonesimulator",
            _ => panic!("unsupported triple: {}", triple),
        })
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[derive(Debug, Clone)]
pub struct MinVersions {
    pub ios: String,
    pub macos: String,
}

impl Default for MinVersions {
    fn default() -> Self {
        Self {
            ios: "10.0".into(),
            macos: "10.10".into(),
        }
    }
}

fn current_triple(triple: &str, min_versions: &MinVersions) -> String {
    match triple {
        "aarch64-apple-darwin" => format!("arm64-apple-macosx{}", &min_versions.macos),
        "aarch64-apple-ios" => format!("arm64-apple-ios{}", &min_versions.ios),
        "aarch64-apple-ios-sim" => format!("arm64-apple-ios{}-simulator", &min_versions.ios),
        "x86_64-apple-darwin" => format!("x86_64-apple-macosx{}", &min_versions.macos),
        "x86_64-apple-ios" => format!("x86_64-apple-ios{}-simulator", &min_versions.ios),
        _ => panic!("unsupported triple: {}", triple),
    }
}
