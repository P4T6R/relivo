use hbb_common::{
    anyhow::{anyhow, Context},
    bail,
    config::{self, Config2},
    sysinfo::System,
    toml, ResultType,
};
use std::{collections::HashMap, fs, path::Path};

fn public_settings() -> ResultType<HashMap<String, String>> {
    let profile: serde_json::Value =
        serde_json::from_str(include_str!("../../../agent/server-profile.json"))?;
    let mut settings = HashMap::from([
        ("management-required".to_owned(), "Y".to_owned()),
        ("allow-websocket".to_owned(), "N".to_owned()),
        ("direct-server".to_owned(), "N".to_owned()),
        ("allow-auto-update".to_owned(), "N".to_owned()),
    ]);
    for (key, field) in [
        ("api-server", "api_origin"),
        ("custom-rendezvous-server", "id_server"),
        ("relay-server", "relay_url"),
        ("key", "server_public_key"),
    ] {
        let value = profile[field]
            .as_str()
            .ok_or_else(|| anyhow!("Invalid packaged Relivo server profile"))?;
        settings.insert(key.to_owned(), value.to_owned());
    }
    Ok(settings)
}

fn credential_options(original: &str, credential: &str) -> ResultType<toml::Table> {
    if credential.len() != 64
        || !credential
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("Expected a unique 64-character lowercase hexadecimal enrollment credential");
    }
    let mut document: toml::Table = toml::from_str(original)
        .map_err(|_| anyhow!("Existing native settings are invalid; no settings were changed"))?;
    let options = document
        .entry("options")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()))
        .as_table_mut()
        .ok_or_else(|| anyhow!("Invalid native options table"))?;
    options.insert(
        "management-device-token".to_owned(),
        toml::Value::String(credential.to_owned()),
    );
    Ok(document)
}

fn import_credential(file: &Path) -> ResultType<()> {
    let executable = std::env::current_exe()?;
    let mut system = System::new();
    system.refresh_processes();
    if system.processes().values().any(|process| {
        process.pid().as_u32() != std::process::id()
            && process.exe().file_name() == executable.file_name()
    }) {
        bail!(
            "Close all native agent instances, including the tray and service, before provisioning"
        );
    }
    let credential = fs::read_to_string(file).context("Read enrollment credential file")?;
    let destination = Config2::file();
    for component in destination.ancestors() {
        if component.is_symlink() {
            bail!("Refusing a symlink in the configuration path");
        }
    }
    let original = match fs::read_to_string(&destination) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error).context("Read native settings"),
    };
    let settings = credential_options(&original, credential.trim())?;
    config::store_path(destination, toml::Value::Table(settings))
        .context("Save enrollment credential")?;
    println!(
        "Relivo credential imported. Start the agent, then verify and approve it in the console."
    );
    Ok(())
}

pub(crate) fn initialize() -> ResultType<bool> {
    config::OVERWRITE_SETTINGS
        .write()
        .unwrap()
        .extend(public_settings()?);
    config::OVERWRITE_LOCAL_SETTINGS
        .write()
        .unwrap()
        .insert("enable-check-update".to_owned(), "N".to_owned());
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--relivo-server-profile" {
        println!("{}", serde_json::to_string(&public_settings()?)?);
        return Ok(true);
    }
    if args
        .first()
        .is_some_and(|arg| arg == "--relivo-enrollment-file")
    {
        if args.len() != 2 {
            bail!("Usage: --relivo-enrollment-file <private-credential-file>");
        }
        import_credential(Path::new(&args[1]))?;
        return Ok(true);
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packaged_settings_require_relivo_without_a_shared_credential() {
        let settings = public_settings().unwrap();
        assert_eq!(settings["management-required"], "Y");
        assert_eq!(settings["api-server"], "https://relivo-desk.duckdns.org");
        assert!(!settings.contains_key("management-device-token"));
    }

    #[test]
    fn credential_import_preserves_unrelated_settings() {
        let original = "serial = 7\n[options]\ncustom = 'retained'\n";
        let settings = credential_options(original, &"a".repeat(64)).unwrap();
        assert_eq!(settings["serial"].as_integer(), Some(7));
        assert_eq!(settings["options"]["custom"].as_str(), Some("retained"));
        let path = std::env::temp_dir().join(format!("relivo-import-{}.toml", std::process::id()));
        config::store_path(path.clone(), toml::Value::Table(settings)).unwrap();
        let saved: toml::Table = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        fs::remove_file(path).unwrap();
        assert_eq!(saved["serial"].as_integer(), Some(7));
        assert_eq!(saved["options"]["custom"].as_str(), Some("retained"));
        assert!(credential_options(original, "invalid").is_err());
        assert!(credential_options("[invalid", &"a".repeat(64)).is_err());
    }
}
