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

fn validate_credential(credential: &str) -> ResultType<()> {
    if credential.len() != 64
        || !credential
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!(
            "Expected a unique 64-character lowercase hexadecimal enrollment credential (the file contains {} characters)",
            credential.chars().count()
        );
    }
    Ok(())
}

// Accept UTF-8 (with or without BOM) and the UTF-16LE files written by Windows PowerShell 5.
fn decode_credential(bytes: &[u8]) -> ResultType<String> {
    let text = match bytes {
        [0xff, 0xfe, rest @ ..] => String::from_utf16(
            &rest.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>(),
        )
        .map_err(|_| anyhow!("The enrollment credential file is not valid UTF-16 text"))?,
        _ => String::from_utf8(bytes.to_vec())
            .map_err(|_| anyhow!("The enrollment credential file is not UTF-8 text"))?,
    };
    let credential = text.trim_start_matches('\u{feff}').trim().to_owned();
    validate_credential(&credential)?;
    Ok(credential)
}

fn credential_options(original: &str, credential: &str) -> ResultType<toml::Table> {
    validate_credential(credential)?;
    let mut document: toml::Table = toml::from_str(original)
        .map_err(|_| anyhow!("Existing native settings are invalid; no settings were changed"))?;
    let options = document
        .entry("options")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()))
        .as_table_mut()
        .ok_or_else(|| anyhow!("Invalid native options table"))?;
    options.insert(
        super::DEVICE_TOKEN.to_owned(),
        toml::Value::String(credential.to_owned()),
    );
    Ok(document)
}

fn import_credential(file: &Path) -> ResultType<String> {
    let bytes = fs::read(file)
        .with_context(|| format!("Read enrollment credential file '{}'", file.display()))?;
    let credential = decode_credential(&bytes)?;
    // The installed service keeps its own profile (LocalService); only its IPC can update it.
    #[cfg(windows)]
    if crate::platform::is_installed() || crate::platform::windows::is_self_service_running() {
        return import_into_service(&credential);
    }
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
    let settings = credential_options(&original, &credential)?;
    config::store_path(destination.clone(), toml::Value::Table(settings))
        .context("Save enrollment credential")?;
    Ok(format!(
        "Relivo credential imported into {}. Start the agent, then verify and approve it in the console.",
        destination.display()
    ))
}

#[cfg(windows)]
const SERVICE_UNAVAILABLE: &str = "The Relivo Agent service is not reachable. Start the Relivo Agent service and try again, or use the enrollment screen of the application (Settings > Relivo enrollment)";

#[cfg(windows)]
fn import_into_service(credential: &str) -> ResultType<String> {
    use crate::ipc::Data;
    let (_, _, _, installed) = crate::platform::windows::get_install_info();
    let current = std::env::current_exe()?;
    if crate::platform::is_installed()
        && fs::canonicalize(&current).ok() != fs::canonicalize(&installed).ok()
    {
        // The service only accepts IPC from its own executable.
        bail!("Relivo Agent is installed; run the installed executable instead: & \"{installed}\" --relivo-enrollment-file <file> | Out-Host");
    }
    if !crate::platform::windows::is_self_service_running() {
        bail!(SERVICE_UNAVAILABLE);
    }
    const TIMEOUT: u64 = 3000;
    let runtime = hbb_common::tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    runtime.block_on(async {
        let mut ipc = crate::ipc::connect(TIMEOUT, "")
            .await
            .map_err(|e| anyhow!("{SERVICE_UNAVAILABLE} ({e})"))?;
        // `Data::Options(Some(..))` replaces every option, so send the complete current map.
        ipc.send(&Data::Options(None)).await?;
        let Ok(Some(Data::Options(Some(mut options)))) = ipc.next_timeout(TIMEOUT).await else {
            bail!("{SERVICE_UNAVAILABLE} (no answer; the service only accepts the installed executable)");
        };
        options.insert(super::DEVICE_TOKEN.to_owned(), credential.to_owned());
        ipc.send(&Data::Options(Some(options))).await?;
        match ipc.next_timeout(TIMEOUT).await {
            Ok(Some(Data::Options(None))) => {}
            Ok(Some(Data::SecurityConfigError(error))) => bail!("The agent service rejected the credential: {error}"),
            _ => bail!("The agent service did not confirm the credential update"),
        }
        ipc.send(&Data::Options(None)).await?;
        match ipc.next_timeout(TIMEOUT).await {
            Ok(Some(Data::Options(Some(saved))))
                if saved.get(super::DEVICE_TOKEN).map(|v| v.trim()) == Some(credential) => {}
            _ => bail!("The agent service did not keep the credential"),
        }
        Ok::<_, hbb_common::anyhow::Error>("Relivo credential imported into the running Relivo Agent service. Verify and approve the device in the console.".to_owned())
    })
}

#[cfg(windows)]
use console::show;
#[cfg(not(windows))]
fn show(error: bool, message: &str) {
    if error {
        eprintln!("{message}");
    } else {
        println!("{message}");
    }
}

/// rustdesk.exe is a GUI-subsystem program: without this, PowerShell drops its output.
#[cfg(windows)]
mod console {
    use std::{ffi::c_void, io::Write};
    use winapi::um::winuser::{MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK};
    type Handle = *mut c_void;
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const STD_ERROR_HANDLE: u32 = -12i32 as u32;
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    const FILE_TYPE_CHAR: u32 = 2;
    const GENERIC_READ_WRITE: u32 = 0x8000_0000 | 0x4000_0000;
    const FILE_SHARE_READ_WRITE: u32 = 1 | 2;
    const OPEN_EXISTING: u32 = 3;
    #[link(name = "kernel32")]
    extern "system" {
        fn AttachConsole(process: u32) -> i32;
        fn GetStdHandle(kind: u32) -> Handle;
        fn SetStdHandle(kind: u32, handle: Handle) -> i32;
        fn GetFileType(handle: Handle) -> u32;
        fn GetConsoleMode(handle: Handle, mode: *mut u32) -> i32;
        fn CreateFileW(name: *const u16, access: u32, share: u32, security: *mut c_void,
            disposition: u32, flags: u32, template: Handle) -> Handle;
    }
    fn valid(handle: Handle) -> bool {
        !handle.is_null() && handle as isize != -1
    }
    // A redirected pipe/file is kept (CI captures stdout); a console handle must be usable.
    fn usable(kind: u32) -> bool {
        let mut mode = 0;
        unsafe {
            let handle = GetStdHandle(kind);
            valid(handle) && (GetFileType(handle) != FILE_TYPE_CHAR || GetConsoleMode(handle, &mut mode) != 0)
        }
    }
    fn attach() -> (bool, bool) {
        let missing: Vec<u32> = [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE]
            .into_iter()
            .filter(|kind| !usable(*kind))
            .collect();
        if !missing.is_empty() && unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } != 0 {
            let name: Vec<u16> = "CONOUT$\0".encode_utf16().collect();
            let handle = unsafe {
                CreateFileW(name.as_ptr(), GENERIC_READ_WRITE, FILE_SHARE_READ_WRITE,
                    std::ptr::null_mut(), OPEN_EXISTING, 0, std::ptr::null_mut())
            };
            // Rust's std re-reads GetStdHandle on every write.
            if valid(handle) {
                for kind in missing {
                    unsafe { SetStdHandle(kind, handle) };
                }
            }
        }
        (usable(STD_OUTPUT_HANDLE), usable(STD_ERROR_HANDLE))
    }
    pub(super) fn show(error: bool, message: &str) {
        match attach() {
            (_, true) if error => eprintln!("{message}"),
            (true, _) => {
                println!("{message}");
                let _ = std::io::stdout().flush();
            }
            _ => {
                // No parent console (started by double-click): show a dialog instead.
                let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
                let (text, caption) = (wide(message), wide("Relivo Agent"));
                let icon = if error { MB_ICONERROR } else { MB_ICONINFORMATION };
                unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), MB_OK | icon) };
            }
        }
    }
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
    let result = if args.len() == 1 && args[0] == "--relivo-server-profile" {
        public_settings().and_then(|settings| Ok(serde_json::to_string(&settings)?))
    } else if args.first().is_some_and(|arg| arg == "--relivo-enrollment-file") {
        if args.len() != 2 {
            Err(anyhow!("Usage: --relivo-enrollment-file <private-credential-file>"))
        } else {
            import_credential(Path::new(&args[1]))
        }
    } else {
        return Ok(false);
    };
    match result {
        Ok(message) => show(false, &message),
        Err(error) => {
            show(true, &format!("Relivo enrollment failed: {error:#}"));
            std::process::exit(1);
        }
    }
    Ok(true)
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

    #[test]
    fn credential_file_is_trimmed_and_decoded() {
        let token = "0123456789abcdef".repeat(4);
        assert_eq!(decode_credential(format!(" {token}\r\n").as_bytes()).unwrap(), token);
        assert_eq!(decode_credential(format!("\u{feff}{token}\n").as_bytes()).unwrap(), token);
        let utf16: Vec<u8> = [0xff, 0xfe].into_iter()
            .chain(format!("\u{feff}{token}\r\n").encode_utf16().skip(1).flat_map(u16::to_le_bytes))
            .collect();
        assert_eq!(decode_credential(&utf16).unwrap(), token);
        assert!(decode_credential(token.to_uppercase().as_bytes()).is_err());
        assert!(decode_credential(&[0xc3, 0x28]).is_err());
    }
}
