use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::config::{Config, expand_tilde};

struct SyncResult {
    created: u32,
    skipped: u32,
    conflicts: u32,
    pruned: u32,
    errors: u32,
}

struct DiscoveredSkill {
    base_name: String,
    prefix: String,
    canonical_path: PathBuf,
    source_name: String,
}

pub fn run_sync(config: &Config) -> anyhow::Result<()> {
    let mut result = SyncResult {
        created: 0,
        skipped: 0,
        conflicts: 0,
        pruned: 0,
        errors: 0,
    };

    let target_dir = expand_tilde(&config.target);
    std::fs::create_dir_all(&target_dir).context("Failed to create target directory")?;

    let skills = discover_all_skills(config, &mut result);
    let resolved = resolve_names(skills, &mut result);
    reconcile_symlinks(&target_dir, &resolved, &mut result);
    prune_stale(&target_dir, &resolved, &mut result);
    print_summary(&result);

    Ok(())
}

fn discover_all_skills(config: &Config, result: &mut SyncResult) -> Vec<DiscoveredSkill> {
    let mut skills = Vec::new();

    for source in &config.sources {
        let source_path = expand_tilde(&source.path);
        let canonical = match source_path.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                eprintln!(
                    "Warning: cannot access source '{}' ({}): {e}",
                    source.name,
                    source_path.display()
                );
                result.errors += 1;
                continue;
            }
        };

        let entries = match std::fs::read_dir(&canonical) {
            Ok(entries) => entries,
            Err(e) => {
                eprintln!(
                    "Warning: cannot read source '{}' ({}): {e}",
                    source.name,
                    canonical.display()
                );
                result.errors += 1;
                continue;
            }
        };

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("Warning: error reading entry in '{}': {e}", source.name);
                    result.errors += 1;
                    continue;
                }
            };

            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let skill_md = path.join("SKILL.md");
            if !skill_md.exists() {
                continue;
            }

            let base_name = entry.file_name().to_string_lossy().to_string();
            let canonical_skill = match path.canonicalize() {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Warning: cannot canonicalize {}: {e}", path.display());
                    result.errors += 1;
                    continue;
                }
            };

            skills.push(DiscoveredSkill {
                base_name,
                prefix: source.prefix().to_string(),
                canonical_path: canonical_skill,
                source_name: source.name.clone(),
            });
        }
    }

    skills
}

fn resolve_names(
    skills: Vec<DiscoveredSkill>,
    result: &mut SyncResult,
) -> HashMap<String, PathBuf> {
    // Pass 1: group by base_name
    let mut groups: HashMap<String, Vec<DiscoveredSkill>> = HashMap::new();
    for skill in skills {
        groups
            .entry(skill.base_name.clone())
            .or_default()
            .push(skill);
    }

    let mut resolved: HashMap<String, PathBuf> = HashMap::new();

    // Pass 2a: process non-conflicted (single-entry) first — they get literal names
    for (base_name, group) in &groups {
        if group.len() == 1 {
            resolved.insert(base_name.clone(), group[0].canonical_path.clone());
        }
    }

    // Pass 2b: process conflicted (multi-entry) — each gets prefix-base_name
    for (base_name, group) in &groups {
        if group.len() <= 1 {
            continue;
        }
        for skill in group {
            let prefixed = format!("{}-{base_name}", skill.prefix);
            match resolved.entry(prefixed.clone()) {
                std::collections::hash_map::Entry::Occupied(_) => {
                    eprintln!(
                        "Warning: prefix collision for '{prefixed}' (source '{}' skipped)",
                        skill.source_name
                    );
                    result.conflicts += 1;
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    eprintln!(
                        "Info: skill '{base_name}' from '{}' will be symlinked as '{prefixed}'",
                        skill.source_name
                    );
                    entry.insert(skill.canonical_path.clone());
                }
            }
        }
    }

    resolved
}

fn reconcile_symlinks(
    target_dir: &Path,
    skills: &HashMap<String, PathBuf>,
    result: &mut SyncResult,
) {
    for (name, target_path) in skills {
        let link_path = target_dir.join(name);

        match link_path.symlink_metadata() {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    let current_target = match std::fs::read_link(&link_path) {
                        Ok(t) => t,
                        Err(e) => {
                            eprintln!("Warning: cannot read symlink {}: {e}", link_path.display());
                            result.errors += 1;
                            continue;
                        }
                    };

                    let current_canonical = current_target
                        .canonicalize()
                        .unwrap_or_else(|_| current_target.clone());

                    if current_canonical == *target_path {
                        result.skipped += 1;
                    } else {
                        eprintln!(
                            "Conflict: {} points to {} (expected {})",
                            link_path.display(),
                            current_canonical.display(),
                            target_path.display()
                        );
                        result.conflicts += 1;
                    }
                } else {
                    eprintln!(
                        "Conflict: {} exists and is not a symlink",
                        link_path.display()
                    );
                    result.conflicts += 1;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                match std::os::unix::fs::symlink(target_path, &link_path) {
                    Ok(()) => result.created += 1,
                    Err(e) => {
                        eprintln!(
                            "Error: failed to create symlink {}: {e}",
                            link_path.display()
                        );
                        result.errors += 1;
                    }
                }
            }
            Err(e) => {
                eprintln!("Warning: cannot stat {}: {e}", link_path.display());
                result.errors += 1;
            }
        }
    }
}

fn prune_stale(target_dir: &Path, skills: &HashMap<String, PathBuf>, result: &mut SyncResult) {
    let entries = match std::fs::read_dir(target_dir) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!(
                "Warning: cannot read target directory {}: {e}",
                target_dir.display()
            );
            result.errors += 1;
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                eprintln!("Warning: error reading target entry: {e}");
                result.errors += 1;
                continue;
            }
        };

        let name = entry.file_name().to_string_lossy().to_string();

        let meta = match entry.path().symlink_metadata() {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Warning: cannot stat {}: {e}", entry.path().display());
                result.errors += 1;
                continue;
            }
        };

        if !meta.file_type().is_symlink() {
            continue;
        }

        if skills.contains_key(&name) {
            continue;
        }

        match std::fs::remove_file(entry.path()) {
            Ok(()) => {
                result.pruned += 1;
            }
            Err(e) => {
                eprintln!(
                    "Warning: failed to remove stale symlink {}: {e}",
                    entry.path().display()
                );
                result.errors += 1;
            }
        }
    }
}

fn print_summary(result: &SyncResult) {
    println!("Sync complete:");
    println!("  Created: {}", result.created);
    println!("  Skipped: {} (already correct)", result.skipped);
    println!("  Conflicts: {}", result.conflicts);
    println!("  Pruned: {}", result.pruned);
    if result.errors > 0 {
        println!("  Errors: {}", result.errors);
    }
}
