// =============================================================================
//        #######
//     ###       ###     F: path.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 16:07:49 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

pub(crate) fn resolve_runtime_path(
    config_dir: &Path,
    raw_path: &str,
    field: &'static str,
) -> Result<PathBuf, RuntimeConfigError> {
    let raw = Path::new(raw_path);
    if raw.is_absolute() {
        return Ok(raw.to_path_buf());
    }
    let normalized = normalize_relative_path(raw, field)?;
    Ok(config_dir.join(normalized))
}

fn normalize_relative_path(
    path: &Path,
    field: &'static str,
) -> Result<PathBuf, RuntimeConfigError> {
    let mut stack: Vec<OsString> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => stack.push(part.to_os_string()),
            Component::ParentDir => {
                if stack.is_empty() {
                    return Err(RuntimeConfigError::InvalidPath(field));
                }
                let _ = stack.pop();
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(RuntimeConfigError::InvalidPath(field));
            }
        }
    }
    if stack.is_empty() {
        return Err(RuntimeConfigError::InvalidPath(field));
    }
    Ok(stack.into_iter().collect())
}

pub(crate) fn sanitize_distributed_default(value: &str) -> String {
    let mut output = String::with_capacity(value.len().max(2));
    let mut previous_dash = false;
    for byte in value.bytes() {
        let character = if byte.is_ascii_uppercase() {
            byte.to_ascii_lowercase() as char
        } else if byte.is_ascii_lowercase() || byte.is_ascii_digit() {
            byte as char
        } else {
            '-'
        };
        if character == '-' {
            if output.is_empty() || previous_dash {
                continue;
            }
            previous_dash = true;
        } else {
            previous_dash = false;
        }
        output.push(character);
    }
    while output.ends_with('-') {
        let _ = output.pop();
    }
    if output.len() < 2 {
        output.push_str("id");
    }
    if output.len() > 80 {
        output.truncate(80);
        while output.ends_with('-') {
            let _ = output.pop();
        }
    }
    if output.len() < 2 {
        "id".to_string()
    } else {
        output
    }
}
