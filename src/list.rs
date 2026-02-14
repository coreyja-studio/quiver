use crate::config::{Config, expand_tilde};

pub fn run_list(config: &Config) -> anyhow::Result<()> {
    let target_dir = expand_tilde(&config.target);

    if !target_dir.exists() {
        println!("Target directory does not exist: {}", target_dir.display());
        return Ok(());
    }

    let mut entries: Vec<_> = std::fs::read_dir(&target_dir)?
        .filter_map(std::result::Result::ok)
        .collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);

    if entries.is_empty() {
        println!("No skills found in {}", target_dir.display());
        return Ok(());
    }

    for entry in &entries {
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();

        match path.symlink_metadata() {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    match std::fs::read_link(&path) {
                        Ok(target) => println!("{name} -> {}", target.display()),
                        Err(_) => println!("{name} (symlink, unreadable)"),
                    }
                } else if meta.is_dir() {
                    println!("{name} (local)");
                } else {
                    println!("{name} (file, not a skill)");
                }
            }
            Err(_) => {
                println!("{name} (unknown)");
            }
        }
    }

    Ok(())
}
