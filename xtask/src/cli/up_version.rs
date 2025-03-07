use std::{env::current_dir, fs, path::Path};

use toml::Value;

use super::{Module, UpVersionArgs};

pub fn exec(args: &UpVersionArgs) {
    println!(
        "Bump up [{:?}] version, is break change: {}",
        args.module, args.break_change
    );

    match args.module {
        Module::Sparreal => up_sparreal(args.break_change),
    }
}

fn up_sparreal(break_change: bool) {
    let workspace_root = current_dir().unwrap();
    let old = get_workspace_version(&workspace_root).unwrap();

    println!("Current version: {}", old);

    let mut split = old.split(".");

    let mut major: usize = split.next().unwrap().parse().unwrap();
    let mut minor: usize = split.next().unwrap().parse().unwrap();
    let mut patch: usize = split.next().unwrap().parse().unwrap();

    if break_change {
        if major == 0 {
            minor += 1;
            patch = 0;
        } else {
            major += 1;
            minor = 0;
            patch = 0;
        }

        let new_version = format!("{}.{}", major, minor);
        let deps = &["sparreal-macros", "sparreal-kernel", "sparreal-rt"];
        update_dep(
            &workspace_root.join("crates").join("sparreal-kernel"),
            deps,
            &new_version,
        );

        update_dep(
            &workspace_root.join("crates").join("sparreal-rt"),
            deps,
            &new_version,
        );

        update_dep(
            &workspace_root.join("crates").join("bare-test"),
            deps,
            &new_version,
        );
    } else {
        patch += 1;
    }

    let new_version = format!("{}.{}.{}", major, minor, patch);
    println!("New version: {}", new_version);

    let cargo_toml_path = workspace_root.join("Cargo.toml");
    let cargo_toml_content = fs::read_to_string(&cargo_toml_path).unwrap();

    let new_cargo_toml_content = cargo_toml_content.replace(
        &format!("version = \"{}\"", old),
        &format!("version = \"{}\"", new_version),
    );

    fs::write(&cargo_toml_path, new_cargo_toml_content).unwrap();
}

fn get_workspace_version(workspace_root: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let cargo_toml_path = workspace_root.join("Cargo.toml");
    let cargo_toml_content = fs::read_to_string(&cargo_toml_path)?;

    let cargo_toml: Value = toml::from_str(&cargo_toml_content)?;

    if let Some(workspace) = cargo_toml.get("workspace") {
        if let Some(package) = workspace.get("package") {
            if let Some(version) = package.get("version") {
                if let Some(version_str) = version.as_str() {
                    return Ok(version_str.to_string());
                }
            }
        }
    }

    Err("Workspace version not found in Cargo.toml".into())
}

fn update_dep(crate_dir: &Path, deps: &[&str], new_version: &str) {
    let cargo_toml_path = crate_dir.join("Cargo.toml");
    let cargo_toml_content = fs::read_to_string(&cargo_toml_path).unwrap();

    let mut cargo_toml: Value = toml::from_str(&cargo_toml_content).unwrap();

    let mut changed = false;

    if let Some(dependencies) = cargo_toml.get_mut("dependencies") {
        for dep in deps {
            if let Some(pkg) = dependencies.get_mut(dep) {
                if let Some(old_version) = pkg.get_mut("version") {
                    println!(
                        "{}: {}->{}",
                        *dep,
                        old_version.as_str().unwrap(),
                        new_version
                    );
                    *old_version = Value::String(new_version.to_string());

                    changed = true;
                }
            }
        }
    }

    if changed {
        let new_cargo_toml_content = toml::to_string(&cargo_toml).unwrap();

        fs::write(cargo_toml_path, new_cargo_toml_content).expect("Failed to write to Cargo.toml");
    }
}
