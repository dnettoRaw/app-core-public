// =============================================================================
//        #######
//     ###       ###     F: backup.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Cold whole-provider backup, verification, restore, and drill commands.

use crate::bootstrap::{load_config, now_ms, BootstrapError};
use appcore_storage::FileStorageProvider;

pub(super) fn run_backup(
    config_path: Option<&str>,
    action: Option<&str>,
    name: Option<&str>,
    source: Option<&str>,
    confirm_restore: bool,
) -> Result<(), BootstrapError> {
    let config = load_config(config_path)?;
    let storage = FileStorageProvider::new(&config.storage_path, &config.backup_path);
    storage.create_dirs()?;
    if let Some(source) = source {
        return backup_single_file(&storage, source);
    }
    let action = action.unwrap_or("create");
    let generated_name = format!("runtime-{}", now_ms());
    let name = name.unwrap_or(&generated_name);
    match action {
        "create" => create(&storage, name),
        "verify" => verify(&storage, name),
        "restore" => restore(&storage, name, confirm_restore),
        "drill" => drill(&storage, name, confirm_restore),
        _ => Err(BootstrapError::Cli(format!(
            "unsupported backup action: {action}"
        ))),
    }
}

fn backup_single_file(storage: &FileStorageProvider, source: &str) -> Result<(), BootstrapError> {
    let backup_name = format!("{}.bak", source.replace('/', "_"));
    storage.backup_file(source, &backup_name)?;
    println!("backup_file_created: {backup_name}");
    Ok(())
}

fn create(storage: &FileStorageProvider, name: &str) -> Result<(), BootstrapError> {
    let backup = storage.create_snapshot_backup(name)?;
    println!("backup_created: {}", backup.name);
    Ok(())
}

fn verify(storage: &FileStorageProvider, name: &str) -> Result<(), BootstrapError> {
    let backup = storage.verify_snapshot_backup(name)?;
    println!("backup_verified: {}", backup.name);
    Ok(())
}

fn restore(
    storage: &FileStorageProvider,
    name: &str,
    confirmed: bool,
) -> Result<(), BootstrapError> {
    require_restore_confirmation(confirmed)?;
    let backup = storage.restore_snapshot_backup(name)?;
    println!("backup_restored: {}", backup.name);
    Ok(())
}

fn drill(storage: &FileStorageProvider, name: &str, confirmed: bool) -> Result<(), BootstrapError> {
    require_restore_confirmation(confirmed)?;
    run_restore_drill(storage, name)?;
    println!("restore_drill_passed: {name}");
    Ok(())
}

fn require_restore_confirmation(confirmed: bool) -> Result<(), BootstrapError> {
    if confirmed {
        return Ok(());
    }
    Err(BootstrapError::Cli(
        "restore requires --confirm-restore and a stopped Runtime service".to_string(),
    ))
}

fn run_restore_drill(storage: &FileStorageProvider, name: &str) -> Result<(), BootstrapError> {
    storage.create_snapshot_backup(name)?;
    storage.verify_snapshot_backup(name)?;
    let probe = format!(".appcore-restore-drill-{}", now_ms());
    storage.write_bytes(&probe, b"restore drill probe")?;
    storage.restore_snapshot_backup(name)?;
    if storage.exists(&probe)? {
        return Err(BootstrapError::Runtime(
            "restore drill retained a post-backup probe".to_string(),
        ));
    }
    storage.verify_snapshot_backup(name)?;
    Ok(())
}
