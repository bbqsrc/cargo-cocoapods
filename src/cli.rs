use std::{
    path::{Path, PathBuf},
    process::{exit, Stdio},
};

use cargo_metadata::{Metadata, MetadataCommand, Package, Target};
use glob::glob;
use gumdrop::{Options, ParsingStyle};
use heck::CamelCase;
use jwalk::WalkDir;

use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{Read, Write},
};

use crate::{
    cmd::{lipo, Ar, Swiftc, Xcodebuild},
    podspec::Podspec,
    IOS_TRIPLES, MACOS_TRIPLES,
};

#[derive(Debug, Options)]
struct BuildArgs {
    #[options(help = "show help information")]
    help: bool,

    #[options(long = "macos", help = "macOS builds only")]
    is_macos: bool,

    #[options(long = "ios", help = "iOS builds only")]
    is_ios: bool,

    #[options(free, help = "args to be passed to `cargo build` step")]
    cargo_args: Vec<String>,

    manifest_path: Option<PathBuf>,
}

#[derive(Debug, Options)]
struct InitArgs {
    #[options(help = "show help information")]
    help: bool,

    #[options(help = "override the name of the pod")]
    name: Option<String>,

    #[options(help = "override the repository url")]
    repo: Option<String>,

    #[options(help = "create a git subtree for the crate")]
    subtree_url: Option<String>,

    #[options(short = "b", help = "branch for the subtree repo")]
    subtree_branch: Option<String>,

    manifest_path: Option<PathBuf>,
}

#[derive(Debug, Options)]
struct PublishArgs {
    #[options(help = "show help information")]
    help: bool,

    #[options(help = "GitHub Personal Access Token")]
    token: Option<String>,

    #[options(help = "URL to repository; will use git remote origin if not given")]
    url: Option<String>,

    #[options(
        no_short,
        help = "Override tag; uses data in .podspec file if not given"
    )]
    tag: Option<String>,

    #[options(help = "Overwrite tag if present")]
    force: bool,
}

#[derive(Debug, Options)]
struct UpdateArgs {
    #[options(help = "show help information")]
    help: bool,

    manifest_path: Option<PathBuf>,
}

#[derive(Debug, Options)]
struct BundleArgs {
    #[options(help = "show help information")]
    help: bool,

    manifest_path: Option<PathBuf>,
}

#[derive(Debug, Options)]
struct ExampleArgs {
    #[options(help = "show help information")]
    help: bool,

    #[options(free)]
    example_args: Vec<String>,
}

#[derive(Debug, Options)]
enum Command {
    Init(InitArgs),
    Build(BuildArgs),
    Bundle(BundleArgs),
    Publish(PublishArgs),
    Update(UpdateArgs),
    #[options(help = "Run example swift (if present)")]
    Example(ExampleArgs),
}

#[derive(Debug, Options)]
pub struct Args {
    #[options(help = "show help information")]
    help: bool,

    #[options(command)]
    command: Option<Command>,
}

fn derive_manifest(manifest_path: Option<&Path>) -> (Metadata, Package, Vec<Target>) {
    let mut cmd = MetadataCommand::new();

    if let Some(path) = manifest_path {
        cmd.manifest_path(path);
    }

    let metadata = match cmd.exec() {
        Ok(v) => v,
        Err(e) => {
            log::error!("Failed to load Cargo.toml.");
            log::error!("{}", e);
            exit(1);
        }
    };
    let packages = metadata
        .packages
        .iter()
        .filter(|p| metadata.workspace_members.contains(&p.id))
        .cloned()
        .collect::<Vec<_>>();

    log::trace!("Got these packages:");
    log::trace!("{:#?}", packages);

    let lib_targets = packages
        .iter()
        .filter_map(|x| {
            let targets = x
                .targets
                .iter()
                .filter(|x| x.kind.contains(&"staticlib".into()))
                .collect::<Vec<_>>();

            if targets.is_empty() {
                return None;
            }

            Some((x, targets))
        })
        .collect::<Vec<_>>();

    if lib_targets.is_empty() {
        log::error!("No lib crates found!");
        exit(1);
    }

    log::trace!("Got these libs:");
    log::trace!("{:#?}", &lib_targets);

    let (package, targets) = lib_targets.first().unwrap();
    (
        metadata,
        (**package).clone(),
        targets.iter().map(|x| (*x).clone()).collect::<Vec<_>>(),
    )
}

fn init_subtree(args: &InitArgs) {
    let subtree_url = args.subtree_url.as_ref().unwrap();
    let branch = args.subtree_branch.as_deref().unwrap_or("main");

    let has_commits = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .code()
        .unwrap()
        == 0;

    if !has_commits {
        let exists = Path::new(".gitignore").exists();

        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(".gitignore")
            .unwrap();

        if !exists {
            writeln!(f, "dist/").unwrap();
        }

        drop(f);

        std::process::Command::new("git")
            .arg("init")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();

        std::process::Command::new("git")
            .args(["reset"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();

        std::process::Command::new("git")
            .args(["add", ".gitignore"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();

        std::process::Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
    }

    std::process::Command::new("git")
        .args(["remote", "add", "-f", "crate", subtree_url])
        .status()
        .unwrap();

    std::process::Command::new("git")
        .args([
            "subtree", "add", "--prefix", "crate", "crate", branch, "--squash",
        ])
        .status()
        .unwrap();

    std::fs::write(".crate-remote", subtree_url).unwrap();

    std::process::Command::new("git")
        .args(["add", ".crate-remote"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();

    std::process::Command::new("git")
        .args(["commit", "-m", "Add .crate-remote"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
}

fn init(args: InitArgs) {
    if args.subtree_url.is_some() {
        init_subtree(&args);
    }

    std::fs::create_dir_all("./src").unwrap();

    let manifest_path = args
        .subtree_url
        .as_ref()
        .map(|_| Path::new("crate/Cargo.toml"))
        .or(args.manifest_path.as_deref());

    let (_metadata, package, targets) = derive_manifest(manifest_path);
    let mut config = crate::meta::config(&package);

    if let Some(name) = args.name {
        config.name = Some(name);
    }

    let mut podspec = Podspec::from(package.clone());
    podspec.disable_bitcode();
    for target in &targets {
        podspec.add_target(target);
    }

    let name = config.name.unwrap_or_else(|| package.name.to_camel_case());
    podspec.name = name.clone();

    log::info!(
        "Writing {}.podspec to {}",
        &name,
        std::env::current_dir().unwrap().display()
    );

    std::fs::write(
        std::env::current_dir()
            .unwrap()
            .join(&name)
            .with_extension("podspec"),
        podspec.to_string(),
    )
    .unwrap();

    std::process::Command::new("git")
        .arg("add")
        .arg(format!("{}.podspec", name))
        .status()
        .unwrap();
}

fn update(_args: UpdateArgs) {
    let has_subtree = std::fs::read_dir("./crate").is_ok();

    if !has_subtree {
        println!("No crate found.");
        std::process::exit(1);
    }

    let crate_remote = std::fs::read_to_string(".crate-remote").unwrap();

    std::process::Command::new("git")
        .args([
            "subtree",
            "pull",
            "--prefix",
            "crate",
            crate_remote.trim(),
            "main",
            "--squash",
        ])
        .status()
        .unwrap();
}

fn build_static_libs(
    mut cargo_args: Vec<String>,
    metadata: &Metadata,
    package: &Package,
    targets: &[Target],
    dist_dir: &Path,
    build_target: BuildTarget,
) {
    let package_dir = package.manifest_path.parent().unwrap();

    if cargo_args.contains(&"--target".into()) {
        log::error!("Do not pass --target to the cargo args, we handle that!");
        exit(1);
    }

    if !cargo_args.contains(&"--release".into()) {
        cargo_args.push("--release".into())
    }

    if !cargo_args.contains(&"--lib".into()) {
        cargo_args.push("--lib".into())
    }

    let mut lib_paths = vec![];

    if build_target.is_ios() {
        for triple in IOS_TRIPLES {
            log::info!("Building for target '{}'...", triple);
            std::fs::create_dir_all(format!("./dist/{}", triple)).unwrap();

            if !crate::cargo::build(package_dir, triple, &cargo_args, false).success() {
                std::process::exit(1);
            }

            for target in targets {
                lib_paths.push((
                    triple,
                    metadata
                        .target_directory
                        .join(triple)
                        .join("release")
                        .join(format!("lib{}.a", target.name.replace('-', "_"))),
                ));
            }
        }
    }

    if build_target.is_macos() {
        for triple in MACOS_TRIPLES {
            log::info!("Building for target '{}'...", triple);
            std::fs::create_dir_all(format!("./dist/{}", triple)).unwrap();

            if !crate::cargo::build(package_dir, triple, &cargo_args, false).success() {
                std::process::exit(1);
            }

            for target in targets {
                lib_paths.push((
                    triple,
                    metadata
                        .target_directory
                        .join(triple)
                        .join("release")
                        .join(format!("lib{}.a", target.name.replace('-', "_"))),
                ));
            }
        }
    }

    for (triple, path) in lib_paths {
        let dest = dist_dir.join(triple).join(path.file_name().unwrap());
        let result = std::fs::copy(&path, &dest);
        match result {
            Ok(_) => {}
            Err(e) => {
                panic!("Error copying {:?} -> {:?}: {:?}", path, dest, e);
            }
        }
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy)]
enum BuildTarget {
    _iOS,
    MacOS,
    Both,
}

impl BuildTarget {
    fn is_ios(&self) -> bool {
        matches!(self, BuildTarget::_iOS | BuildTarget::Both)
    }

    fn is_macos(&self) -> bool {
        matches!(self, BuildTarget::MacOS | BuildTarget::Both)
    }

    fn triples(&self) -> impl Iterator<Item = &'_ str> {
        const MAC: &[&str] = &["aarch64-apple-darwin", "x86_64-apple-darwin"];
        const IOS: &[&str] = &[
            "aarch64-apple-ios",
            "aarch64-apple-ios-sim",
            "x86_64-apple-ios",
        ];
        IOS.iter()
            .filter(|_| self.is_ios())
            .chain(MAC.iter().filter(|_| self.is_macos()))
            .copied()
    }

    fn framework_targets(&self) -> impl Iterator<Item = &'_ str> {
        const MAC: &[&str] = &["macos-universal"];
        const IOS: &[&str] = &["aarch64-apple-ios", "ios-simulator"];
        IOS.iter()
            .filter(|_| self.is_ios())
            .chain(MAC.iter().filter(|_| self.is_macos()))
            .copied()
    }
}

fn build_safe_frameworks(
    package: &Package,
    targets: &[Target],
    dist_dir: &Path,
    build_target: BuildTarget,
) {
    let package_dir = package.manifest_path.parent().unwrap();
    let bindings_path = package_dir.join("bindings");

    let swift_files = WalkDir::new(&bindings_path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.path())
        .collect::<Vec<_>>();

    for target in targets {
        let sys_name = target.name.replace('-', "_");
        let ffi_mod_name = format!("{sys_name}_ffi").to_camel_case();
        let ffi_fw_name = format!("{ffi_mod_name}.framework");

        let mod_name = target.name.replace('-', "_").to_string().to_camel_case();
        let fw_name = format!("{mod_name}.framework");

        for triple in build_target.triples() {
            let triple_dir = dist_dir.join(triple);
            let ffi_fw_dir = triple_dir.join(&ffi_fw_name);
            let fw_dir = triple_dir.join(&fw_name);

            std::fs::create_dir_all(&fw_dir).unwrap();
            dircpy::copy_dir(&ffi_fw_dir, &fw_dir).unwrap();
            std::fs::rename(fw_dir.join("Headers"), fw_dir.join("PrivateHeaders")).unwrap();
            std::fs::rename(fw_dir.join(&ffi_mod_name), fw_dir.join(&mod_name)).unwrap();
            std::fs::write(
                fw_dir.join("Modules").join("module.modulemap"),
                format!(
                    "framework module {mod_name} {{
}}"
                ),
            )
            .unwrap();

            std::fs::write(
                fw_dir.join("Modules").join("module.private.modulemap"),
                format!(
                    "framework module {mod_name}_Private {{
    header \"{sys_name}.h\"
    link \"{mod_name}\"
}}"
                ),
            )
            .unwrap();

            // Build the bindings
            let obj_path = Swiftc::build(
                triple,
                &Default::default(),
                &mod_name,
                &triple_dir,
                &swift_files,
            );
            Ar::insert(&fw_dir.join(&mod_name), &obj_path);
            let swift_mod_path = fw_dir
                .join("Modules")
                .join(format!("{mod_name}.swiftmodule"));
            std::fs::create_dir_all(&swift_mod_path).unwrap();
            let arch = current_arch(triple);
            for ext in [
                "swiftdoc",
                "swiftmodule",
                "swiftsourceinfo",
                "abi.json",
                "swiftinterface",
            ] {
                std::fs::rename(
                    format!("{mod_name}.{ext}"),
                    swift_mod_path.join(format!("{arch}.{ext}")),
                )
                .unwrap();
            }
            log::debug!("Deleting {}", &obj_path);
            std::fs::remove_file(obj_path).unwrap();
            std::fs::remove_file(format!("{mod_name}.private.swiftinterface")).unwrap();
        }

        if build_target.is_ios() {
            let output_path = dist_dir.join("ios-simulator").join(&fw_name);
            std::fs::create_dir_all(&output_path).unwrap();
            let lipo_1 = dist_dir
                .join("aarch64-apple-ios-sim")
                .join(&fw_name)
                .join(&mod_name);
            let lipo_2 = dist_dir
                .join("x86_64-apple-ios")
                .join(&fw_name)
                .join(&mod_name);

            lipo([lipo_1, lipo_2].iter(), &output_path.join(&mod_name)).unwrap();

            dircpy::copy_dir(
                dist_dir
                    .join("aarch64-apple-ios-sim")
                    .join(&fw_name)
                    .join("PrivateHeaders"),
                output_path.join("PrivateHeaders"),
            )
            .unwrap();
            dircpy::copy_dir(
                dist_dir
                    .join("x86_64-apple-ios")
                    .join(&fw_name)
                    .join("Modules"),
                output_path.join("Modules"),
            )
            .unwrap();
            dircpy::copy_dir(
                dist_dir
                    .join("aarch64-apple-ios-sim")
                    .join(&fw_name)
                    .join("Modules"),
                output_path.join("Modules"),
            )
            .unwrap();
        }

        if build_target.is_macos() {
            let output_path = dist_dir.join("macos-universal").join(&fw_name);
            std::fs::create_dir_all(&output_path).unwrap();
            lipo(
                [
                    dist_dir
                        .join("aarch64-apple-darwin")
                        .join(&fw_name)
                        .join(&mod_name),
                    dist_dir
                        .join("x86_64-apple-darwin")
                        .join(&fw_name)
                        .join(&mod_name),
                ]
                .iter(),
                &output_path.join(&mod_name),
            )
            .unwrap();

            dircpy::copy_dir(
                dist_dir
                    .join("aarch64-apple-darwin")
                    .join(&fw_name)
                    .join("PrivateHeaders"),
                output_path.join("PrivateHeaders"),
            )
            .unwrap();
            dircpy::copy_dir(
                dist_dir
                    .join("x86_64-apple-darwin")
                    .join(&fw_name)
                    .join("Modules"),
                output_path.join("Modules"),
            )
            .unwrap();
            dircpy::copy_dir(
                dist_dir
                    .join("aarch64-apple-darwin")
                    .join(&fw_name)
                    .join("Modules"),
                output_path.join("Modules"),
            )
            .unwrap();
        }

        Xcodebuild::create_xcframework_frameworks(
            &mod_name,
            build_target
                .framework_targets()
                .map(|x| dist_dir.join(x).join(format!("{mod_name}.framework"))),
            dist_dir,
        )
        .unwrap();

        Xcodebuild::create_xcframework_frameworks(
            &ffi_mod_name,
            build_target
                .framework_targets()
                .map(|x| dist_dir.join(x).join(format!("{ffi_mod_name}.framework"))),
            dist_dir,
        )
        .unwrap();
    }
}

fn current_arch(triple: &str) -> &str {
    if triple.starts_with("aarch64-") {
        return "arm64";
    }

    if triple.starts_with("x86_64-") {
        return "x86_64";
    }

    panic!("unsupported triple: {}", triple);
}

fn build_ffi_frameworks(
    package: &Package,
    targets: &[Target],
    dist_dir: &Path,
    build_target: BuildTarget,
) {
    let package_dir = package.manifest_path.parent().unwrap();
    let headers_path = package_dir.join("headers");

    for target in targets {
        let sys_name = target.name.replace('-', "_");
        let mod_name = format!("{sys_name}_ffi").to_camel_case();
        let fw_name = format!("{mod_name}.framework");

        for triple in build_target.triples() {
            let triple_dir = dist_dir.join(triple);
            let fw_dir = triple_dir.join(&fw_name);

            let headers_dir = fw_dir.join("Headers");
            std::fs::create_dir_all(&fw_dir).unwrap();
            std::fs::create_dir_all(&headers_dir).unwrap();
            std::fs::create_dir_all(&fw_dir.join("Modules")).unwrap();

            dircpy::copy_dir(&headers_path, &headers_dir).unwrap();

            std::fs::copy(
                triple_dir.join(format!("lib{sys_name}.a")),
                fw_dir.join(&mod_name),
            )
            .unwrap();

            std::fs::write(
                fw_dir.join("Modules").join("module.modulemap"),
                format!(
                    "framework module {mod_name} {{
    header \"{sys_name}.h\"
    link \"{mod_name}\"
}}"
                ),
            )
            .unwrap();
        }

        if build_target.is_ios() {
            let output_path = dist_dir.join("ios-simulator").join(&fw_name);
            std::fs::create_dir_all(&output_path).unwrap();
            lipo(
                [
                    dist_dir
                        .join("aarch64-apple-ios-sim")
                        .join(&fw_name)
                        .join(&mod_name),
                    dist_dir
                        .join("x86_64-apple-ios")
                        .join(&fw_name)
                        .join(&mod_name),
                ]
                .iter(),
                &output_path.join(&mod_name),
            )
            .unwrap();

            dircpy::copy_dir(
                dist_dir
                    .join("aarch64-apple-ios-sim")
                    .join(&fw_name)
                    .join("Headers"),
                output_path.join("Headers"),
            )
            .unwrap();
            dircpy::copy_dir(
                dist_dir
                    .join("aarch64-apple-ios-sim")
                    .join(&fw_name)
                    .join("Modules"),
                output_path.join("Modules"),
            )
            .unwrap();
        }

        if build_target.is_macos() {
            let output_path = dist_dir.join("macos-universal").join(&fw_name);
            std::fs::create_dir_all(&output_path).unwrap();
            lipo(
                [
                    dist_dir
                        .join("aarch64-apple-darwin")
                        .join(&fw_name)
                        .join(&mod_name),
                    dist_dir
                        .join("x86_64-apple-darwin")
                        .join(&fw_name)
                        .join(&mod_name),
                ]
                .iter(),
                &output_path.join(&mod_name),
            )
            .unwrap();

            dircpy::copy_dir(
                dist_dir
                    .join("aarch64-apple-darwin")
                    .join(&fw_name)
                    .join("Headers"),
                output_path.join("Headers"),
            )
            .unwrap();
            dircpy::copy_dir(
                dist_dir
                    .join("aarch64-apple-darwin")
                    .join(&fw_name)
                    .join("Modules"),
                output_path.join("Modules"),
            )
            .unwrap();
        }
    }
}

fn build(args: BuildArgs) {
    let has_subtree = std::fs::read_dir("./crate").is_ok();
    let (metadata, package, targets) = derive_manifest(if has_subtree {
        Some(Path::new("./crate/Cargo.toml"))
    } else {
        args.manifest_path.as_deref()
    });

    let dist_dir = if has_subtree {
        Path::new("./dist").to_path_buf()
    } else {
        Path::new(&metadata.target_directory)
            .parent()
            .unwrap()
            .join("dist")
    };
    std::fs::create_dir_all(&dist_dir).unwrap();

    let build_target = match (args.is_ios, args.is_macos) {
        (true, true) | (false, false) => BuildTarget::Both,
        (true, false) => BuildTarget::_iOS,
        (false, true) => BuildTarget::MacOS,
    };

    build_static_libs(
        args.cargo_args,
        &metadata,
        &package,
        &targets,
        &dist_dir,
        build_target,
    );

    build_ffi_frameworks(&package, &targets, &dist_dir, build_target);
    build_safe_frameworks(&package, &targets, &dist_dir, build_target);
}

fn bundle(_args: BundleArgs) {
    let mut builder = globset::GlobSetBuilder::new();
    builder.add(globset::Glob::new("*.podspec").unwrap());
    builder.add(globset::Glob::new("LICENSE").unwrap());
    builder.add(globset::Glob::new("LICENSE*").unwrap());
    builder.add(globset::Glob::new("README").unwrap());
    builder.add(globset::Glob::new("README*").unwrap());
    let set = builder.build().unwrap();

    let cur = std::env::current_dir().unwrap();
    let files = std::fs::read_dir(&cur)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|x| set.is_match(x.path()))
        .map(|x| x.path().strip_prefix(&cur).unwrap().to_path_buf());

    std::process::Command::new("tar")
        .arg("zcvf")
        .arg("cargo-pod.tgz")
        .args(files)
        .args(["src", "dist"])
        .status()
        .unwrap();
}

#[derive(Debug, Deserialize)]
struct ReleaseResponse {
    url: String,
    upload_url: String,
    id: u32,
    tag_name: String,
}

#[derive(Debug, Serialize)]
struct ReleaseRequest {
    tag_name: String,
}

async fn publish(args: PublishArgs) {
    if args.token.is_none() {
        log::error!("You must provide a GitHub access token");
        std::process::exit(1);
    }
    if args.tag.is_none() {
        log::error!("You must provide a tag name");
        std::process::exit(1);
    }
    let tag = args.tag.unwrap();

    let api_url: &str = "https://api.github.com/";
    let mut header_map = reqwest::header::HeaderMap::new();
    let mut auth_value =
        reqwest::header::HeaderValue::from_str(format!("token {}", args.token.unwrap()).as_str())
            .unwrap();
    auth_value.set_sensitive(true);
    header_map.insert(reqwest::header::AUTHORIZATION, auth_value);
    header_map.insert(
        "user-agent",
        reqwest::header::HeaderValue::from_static("cargo-cocoapods"),
    );
    let api_client = reqwest::Client::builder()
        .default_headers(header_map)
        .build()
        .unwrap();

    let repo_url: String = if let Some(u) = args.url {
        u
    } else {
        String::from_utf8(
            std::process::Command::new("git")
                .args(["remote", "get-url", "origin"])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string()
    };
    log::trace!("Derived repo URL {:?}", repo_url);

    let repo_tail: String = {
        let s = repo_url.as_str();
        let git_tail = if s.starts_with("git@github") {
            let (_, tail) = s.split_once(':').unwrap();
            tail
        } else if s.starts_with("https://github.com/") {
            let (_, tail) = s.split_at("https://github.com/".len());
            tail
        } else {
            panic!("Could not parse the repo url {:?}", repo_url);
        };
        let (head, _) = git_tail.split_at(git_tail.len() - 4);
        head.to_string()
    };
    log::trace!("Derived repo tail {:?}", repo_tail);

    log::info!("Getting current releases...");

    let current_releases: Vec<ReleaseResponse> = api_client
        .get(format!("{}repos/{}/releases", api_url, repo_tail))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let relevant_release: Vec<ReleaseResponse> = current_releases
        .into_iter()
        .filter(|r| r.tag_name == tag)
        .collect();

    let release_id: u32 = match relevant_release.get(0) {
        Some(release) => release.id,
        None => 0,
    };

    if release_id != 0 {
        if args.force {
            log::info!("Deleting release...");
            api_client
                .delete(format!(
                    "{}repos/{}/releases/{}",
                    api_url, repo_tail, release_id
                ))
                .send()
                .await
                .unwrap();
        } else {
            log::error!(
                "Tag {} already exists at release {}",
                tag,
                relevant_release.get(0).unwrap().url
            );
            std::process::exit(1);
        }
    }

    let args = ReleaseRequest { tag_name: tag };
    log::info!("Creating new release...");
    let new_release: ReleaseResponse = api_client
        .post(format!("{}repos/{}/releases", api_url, repo_tail))
        .json(&args)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let mut asset_data: Vec<u8> = Vec::new();
    File::open("cargo-pod.tgz")
        .unwrap()
        .read_to_end(&mut asset_data)
        .unwrap();

    log::info!("Uploading cargo-pod.tgz...");
    api_client
        .post({
            let (head, _) = new_release.upload_url.as_str().split_once('{').unwrap();
            head.to_string()
        })
        .body(asset_data)
        .query(&[("name", "cargo-pod.tgz")])
        .header("content-type", "application/x-gtar")
        .send()
        .await
        .unwrap();
}

fn example(args: ExampleArgs) {
    // swiftc example/**/*.swift src/**/*.swift -import-objc-header src/DivvunSpell/divvunspell.h \
    // -L dist/aarch64-apple-darwin -ldivvunspell -o test
    let tempdir = tempfile::tempdir().unwrap();

    let dist_dir = format!("dist/{}-apple-darwin", std::env::consts::ARCH);

    let headers = glob::glob("src/**/*.h")
        .unwrap()
        .filter_map(Result::ok)
        .flat_map(|x| {
            vec![
                "-import-objc-header".to_string(),
                x.to_string_lossy().to_string(),
            ]
        })
        .collect::<Vec<_>>();

    let libs = glob(&format!("{}/lib*.a", &dist_dir))
        .unwrap()
        .filter_map(Result::ok)
        .map(|x| {
            format!(
                "-l{}",
                x.file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .chars()
                    .skip(3)
                    .collect::<String>()
            )
        })
        .collect::<Vec<_>>();

    log::debug!("Headers: {:?}", &headers);
    log::debug!("Libs: {:?}", &libs);

    let example_bin = tempdir.path().join("example");

    let swift_example = glob("example/**/*.swift")
        .unwrap()
        .filter_map(Result::ok)
        .collect::<Vec<PathBuf>>();
    let swift_src = glob("src/**/*.swift")
        .unwrap()
        .filter_map(Result::ok)
        .collect::<Vec<_>>();

    let mut cmd = std::process::Command::new("swiftc");
    cmd.args(swift_example)
        .args(swift_src)
        .args(headers)
        .arg("-L")
        .arg(dist_dir)
        .args(libs)
        .arg("-o")
        .arg(&example_bin);

    log::trace!("Calling: {:?}", &cmd);
    cmd.status().unwrap();

    std::process::Command::new(example_bin)
        .args(args.example_args)
        .status()
        .unwrap();
}

fn print_help(args: &Args) {
    let mut command = args as &dyn Options;
    let mut command_str = String::new();

    while let Some(new_command) = command.command() {
        command = new_command;

        if let Some(name) = new_command.command_name() {
            command_str.push(' ');
            command_str.push_str(name);
        }
    }

    println!("cargo-cocoapods -- Brendan Molloy <https://github.com/bbqsrc/cargo-cocoapods>");
    println!();
    println!("Usage: cargo pod{} [OPTIONS]", command_str);

    if let Some(cmds) = command.self_command_list() {
        println!();
        println!("Subcommands:");
        println!("{}", cmds);
    }
    println!();
    println!("{}", command.self_usage());
}

fn parse_args_or_exit(args: &[&str]) -> Args {
    let all_options_args = Args::parse_args(args, ParsingStyle::AllOptions);
    let free_args = Args::parse_args(args, ParsingStyle::StopAtFirstFree);

    let args = all_options_args.or(free_args).unwrap_or_else(|e| {
        eprintln!("cargo-pod: {}", e);
        exit(2);
    });

    // let args = Args::parse_args(args, ParsingStyle::StopAtFirstFree).unwrap_or_else(|e| {
    //     eprintln!("cargo-pod: {}", e);
    //     exit(2);
    // });

    if args.help_requested() {
        print_help(&args);
        exit(0);
    }

    args
}

pub(crate) async fn run(args: Vec<String>) {
    log::trace!("Args: {:?}", args);

    let args = parse_args_or_exit(&args.iter().map(|x| &**x).collect::<Vec<_>>());
    let command = match args.command {
        Some(v) => v,
        None => {
            print_help(&args);
            exit(0);
        }
    };

    match command {
        Command::Init(args) => init(args),
        Command::Build(args) => build(args),
        Command::Publish(args) => publish(args).await,
        Command::Bundle(args) => bundle(args),
        Command::Update(args) => update(args),
        Command::Example(args) => example(args),
    }
}
