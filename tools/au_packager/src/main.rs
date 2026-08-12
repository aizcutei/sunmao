use std::env;
use std::fs;
use std::path::{Path, PathBuf};

struct Args {
    binary: PathBuf,
    out: PathBuf,
    name: String,
    bundle_id: String,
    component_type: String,
    component_subtype: String,
    component_manufacturer: String,
    version: String,
    factory: String,
    sandbox_safe: bool,
    executable: Option<String>,
    codesign: bool,
    install_dir: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = env::args().skip(1);
    let mut binary = None;
    let mut out = None;
    let mut name = None;
    let mut bundle_id = None;
    let mut component_type = None;
    let mut component_subtype = None;
    let mut component_manufacturer = None;
    let mut version = None;
    let mut factory = Some("RustAUFactory".to_string());
    let mut sandbox_safe = true;
    let mut executable = None;
    let mut codesign = false;
    let mut install_dir = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--binary" => binary = args.next().map(PathBuf::from),
            "--out" => out = args.next().map(PathBuf::from),
            "--name" => name = args.next(),
            "--bundle-id" => bundle_id = args.next(),
            "--type" => component_type = args.next(),
            "--subtype" => component_subtype = args.next(),
            "--manufacturer" => component_manufacturer = args.next(),
            "--version" => version = args.next(),
            "--factory" => factory = args.next(),
            "--sandbox-safe" => sandbox_safe = true,
            "--unsafe" => sandbox_safe = false,
            "--executable" => executable = args.next(),
            "--codesign" => codesign = true,
            "--install" => install_dir = args.next().map(PathBuf::from),
            "--help" => return Err(usage()),
            other => return Err(format!("Unknown arg: {other}\n{}", usage())),
        }
    }

    let args = Args {
        binary: binary.ok_or_else(|| format!("--binary is required\n{}", usage()))?,
        out: out.ok_or_else(|| format!("--out is required\n{}", usage()))?,
        name: name.ok_or_else(|| format!("--name is required\n{}", usage()))?,
        bundle_id: bundle_id.ok_or_else(|| format!("--bundle-id is required\n{}", usage()))?,
        component_type: component_type.ok_or_else(|| format!("--type is required\n{}", usage()))?,
        component_subtype: component_subtype
            .ok_or_else(|| format!("--subtype is required\n{}", usage()))?,
        component_manufacturer: component_manufacturer
            .ok_or_else(|| format!("--manufacturer is required\n{}", usage()))?,
        version: version.ok_or_else(|| format!("--version is required\n{}", usage()))?,
        factory: factory.ok_or_else(|| format!("--factory is required\n{}", usage()))?,
        sandbox_safe,
        executable,
        codesign,
        install_dir,
    };

    Ok(args)
}

fn usage() -> String {
    "Usage: packager \
  --binary <path> \
  --out <path> \
  --name <name> \
  --bundle-id <id> \
  --type <fourcc> \
  --subtype <fourcc> \
  --manufacturer <fourcc> \
  --version <hex> \
  [--factory <symbol>] \
  [--sandbox-safe|--unsafe] \
  [--executable <name>] \
  [--codesign] \
  [--install <dir>]\n"
        .to_string()
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("Error: au_packager is only supported on macOS.");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    };

    if let Err(err) = package_component(&args) {
        eprintln!("Packaging failed: {err}");
        std::process::exit(1);
    }
}

fn package_component(args: &Args) -> Result<(), String> {
    let out_bundle = if args.out.extension().and_then(|s| s.to_str()) == Some("component") {
        args.out.clone()
    } else {
        let mut path = args.out.clone();
        path.set_extension("component");
        path
    };

    let executable_name = args
        .executable
        .clone()
        .or_else(|| {
            args.binary
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .ok_or_else(|| "Could not derive executable name".to_string())?;

    let contents_dir = out_bundle.join("Contents");
    let macos_dir = contents_dir.join("MacOS");
    let resources_dir = contents_dir.join("Resources");

    fs::create_dir_all(&macos_dir).map_err(|err| err.to_string())?;
    fs::create_dir_all(&resources_dir).map_err(|err| err.to_string())?;

    let target_binary = macos_dir.join(&executable_name);
    fs::copy(&args.binary, &target_binary).map_err(|err| err.to_string())?;

    let plist = render_plist(args, &executable_name)?;
    fs::write(contents_dir.join("Info.plist"), plist).map_err(|err| err.to_string())?;

    // Write PkgInfo file for proper bundle recognition
    fs::write(contents_dir.join("PkgInfo"), "BNDL????").map_err(|err| err.to_string())?;

    if args.codesign {
        codesign_bundle(&out_bundle)?;
    }

    if let Some(install_dir) = args.install_dir.as_ref() {
        install_bundle(&out_bundle, install_dir, args.codesign)?;
    }

    Ok(())
}

fn codesign_bundle(bundle: &Path) -> Result<(), String> {
    let status = std::process::Command::new("codesign")
        .arg("--force")
        .arg("--deep")
        .arg("--sign")
        .arg("-")
        .arg(bundle)
        .status()
        .map_err(|err| format!("Failed to run codesign: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("codesign failed with status: {status}"))
    }
}

fn install_bundle(bundle: &Path, install_dir: &Path, codesign: bool) -> Result<(), String> {
    let bundle_name = bundle
        .file_name()
        .ok_or_else(|| "Bundle path has no file name".to_string())?;
    let target = install_dir.join(bundle_name);
    let status = std::process::Command::new("ditto")
        .arg(bundle)
        .arg(&target)
        .status()
        .map_err(|err| format!("Failed to run ditto: {err}"))?;
    if !status.success() {
        return Err(format!("ditto failed with status: {status}"));
    }
    if codesign {
        codesign_bundle(&target)?;
    }
    Ok(())
}

fn render_plist(args: &Args, executable_name: &str) -> Result<String, String> {
    let template_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Info.plist.template");
    let template = fs::read_to_string(&template_path).map_err(|err| err.to_string())?;

    let version_number = parse_version(&args.version)?;
    let plist = template
        .replace("{{BUNDLE_ID}}", &args.bundle_id)
        .replace("{{BUNDLE_NAME}}", &args.name)
        .replace("{{BUNDLE_VERSION}}", &args.version)
        .replace("{{EXECUTABLE_NAME}}", executable_name)
        .replace("{{COMPONENT_TYPE}}", &args.component_type)
        .replace("{{COMPONENT_SUBTYPE}}", &args.component_subtype)
        .replace("{{COMPONENT_MANUFACTURER}}", &args.component_manufacturer)
        .replace("{{FACTORY_FUNCTION}}", &args.factory)
        .replace("{{COMPONENT_NAME}}", &args.name)
        .replace("{{VERSION_NUMBER}}", &version_number.to_string())
        .replace(
            "{{SANDBOX_SAFE}}",
            if args.sandbox_safe {
                "<true/>"
            } else {
                "<false/>"
            },
        );

    Ok(plist)
}

fn parse_version(version: &str) -> Result<u32, String> {
    let trimmed = version.trim().replace('_', "");
    if let Some(hex) = trimmed.strip_prefix("0x") {
        u32::from_str_radix(hex, 16).map_err(|err| err.to_string())
    } else {
        trimmed.parse::<u32>().map_err(|err| err.to_string())
    }
}
