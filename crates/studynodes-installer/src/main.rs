#![cfg_attr(all(target_os = "windows", not(test)), windows_subsystem = "windows")]

//! Self-contained per-user Windows installer & uninstaller for StudyNodes.
//!
//! Installs to `%LocalAppData%\Programs\StudyNodes\`, creates `<Documents>\StudyNodes\`,
//! adds Start Menu & Desktop shortcuts, and registers Add/Remove Programs (`HKCU`) with
//! zero UAC elevation required.

use std::path::{Path, PathBuf};

const APP_NAME: &str = "StudyNodes";
const APP_PUBLISHER: &str = "StudyNodes";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const APP_EXE_NAME: &str = "studynodes.exe";
const SETUP_EXE_NAME: &str = "studynodes-setup.exe";
const ICON_FILE_NAME: &str = "StudyNodes.ico";
const APP_ICO_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/studynodes.ico"));

#[cfg(feature = "embed-app")]
const EMBEDDED_APP_EXE: Option<&[u8]> = Some(include_bytes!("../../../target/release/studynodes.exe"));

#[cfg(not(feature = "embed-app"))]
const EMBEDDED_APP_EXE: Option<&[u8]> = None;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Action {
    Install,
    Uninstall,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SetupConfig {
    action: Action,
    silent: bool,
    no_launch: bool,
    install_dir_override: Option<PathBuf>,
}

fn parse_args<I, S>(args: I) -> SetupConfig
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut cfg = SetupConfig {
        action: Action::Install,
        silent: false,
        no_launch: false,
        install_dir_override: None,
    };
    let mut iter = args.into_iter();
    while let Some(raw) = iter.next() {
        let arg = raw.as_ref();
        match arg.to_ascii_lowercase().as_str() {
            "--uninstall" | "/u" | "-u" => cfg.action = Action::Uninstall,
            "--install" | "/i" => cfg.action = Action::Install,
            "--silent" | "/s" | "-s" => cfg.silent = true,
            "--no-launch" | "/nolaunch" => cfg.no_launch = true,
            "--dir" | "/d" => {
                if let Some(next) = iter.next() {
                    cfg.install_dir_override = Some(PathBuf::from(next.as_ref()));
                }
            }
            _ => {}
        }
    }
    cfg
}

fn default_install_dir() -> PathBuf {
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        PathBuf::from(local).join("Programs").join(APP_NAME)
    } else if let Some(profile) = std::env::var_os("USERPROFILE") {
        PathBuf::from(profile)
            .join("AppData")
            .join("Local")
            .join("Programs")
            .join(APP_NAME)
    } else {
        std::env::temp_dir().join(APP_NAME)
    }
}

/// Write `studynodes.exe`, `StudyNodes.ico`, and copy the setup binary into `install_dir`,
/// and ensure `projects_dir` exists. Pure filesystem logic, safe to run in tests.
fn stage_install_files(
    install_dir: &Path,
    projects_dir: &Path,
    app_binary: &[u8],
    current_setup_exe: Option<&Path>,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(install_dir)
        .map_err(|e| format!("Failed to create install folder {}: {e}", install_dir.display()))?;
    std::fs::create_dir_all(projects_dir)
        .map_err(|e| format!("Failed to create projects folder {}: {e}", projects_dir.display()))?;

    let app_exe_dst = install_dir.join(APP_EXE_NAME);
    let tmp_app_dst = install_dir.join("studynodes.exe.tmp");
    std::fs::write(&tmp_app_dst, app_binary)
        .map_err(|e| format!("Failed writing {}: {e}", tmp_app_dst.display()))?;
    if app_exe_dst.exists() {
        let _ = std::fs::remove_file(&app_exe_dst);
    }
    std::fs::rename(&tmp_app_dst, &app_exe_dst)
        .map_err(|e| format!("Failed replacing {} (is StudyNodes currently running?): {e}", app_exe_dst.display()))?;

    let ico_dst = install_dir.join(ICON_FILE_NAME);
    std::fs::write(&ico_dst, APP_ICO_BYTES)
        .map_err(|e| format!("Failed writing {}: {e}", ico_dst.display()))?;

    if let Some(setup_src) = current_setup_exe {
        let setup_dst = install_dir.join(SETUP_EXE_NAME);
        if setup_src != setup_dst {
            let _ = std::fs::copy(setup_src, &setup_dst);
        }
    }

    Ok(app_exe_dst)
}

/// Remove installed binaries and icon from `install_dir` (never touches `Documents\StudyNodes`).
fn remove_staged_files(install_dir: &Path) -> Result<(), String> {
    for file in [APP_EXE_NAME, ICON_FILE_NAME, "studynodes.exe.old", "studynodes.exe.new"] {
        let p = install_dir.join(file);
        if p.exists() {
            let _ = std::fs::remove_file(&p);
        }
    }
    // Try removing setup binary and folder directly (succeeds if running from another location).
    let setup_p = install_dir.join(SETUP_EXE_NAME);
    if setup_p.exists() {
        let _ = std::fs::remove_file(&setup_p);
    }
    let _ = std::fs::remove_dir(install_dir);
    Ok(())
}

fn load_app_binary() -> Result<Vec<u8>, String> {
    if let Some(embedded) = EMBEDDED_APP_EXE {
        return Ok(embedded.to_vec());
    }
    if let Ok(cur_exe) = std::env::current_exe() {
        if let Some(dir) = cur_exe.parent() {
            let sibling = dir.join(APP_EXE_NAME);
            if sibling.is_file() {
                return std::fs::read(&sibling)
                    .map_err(|e| format!("Failed reading {}: {e}", sibling.display()));
            }
        }
    }
    Err(
        "This installer binary was built without `--features embed-app` and no sibling `studynodes.exe` was found."
            .into(),
    )
}

#[cfg(target_os = "windows")]
mod win {
    use super::*;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoTaskMemFree, CoUninitialize, IPersistFile,
    };
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, REG_DWORD, REG_SZ, RegCloseKey, RegCreateKeyW, RegDeleteTreeW,
        RegSetValueExW,
    };
    use windows::Win32::UI::Shell::{
        FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Programs, IShellLinkW, KF_FLAG_DEFAULT,
        SHGetKnownFolderPath, ShellLink,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONQUESTION, MB_OK, MB_YESNO, MessageBoxW,
    };
    use windows_core::{Interface, PCWSTR};

    const UNINSTALL_REG_KEY: &str =
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall\StudyNodes";

    fn to_wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(Some(0)).collect()
    }

    pub fn msg_confirm(title: &str, body: &str) -> bool {
        let t = to_wide(title);
        let b = to_wide(body);
        let res = unsafe {
            MessageBoxW(
                Some(HWND::default()),
                PCWSTR(b.as_ptr()),
                PCWSTR(t.as_ptr()),
                MB_YESNO | MB_ICONQUESTION,
            )
        };
        res == IDYES
    }

    pub fn msg_info(title: &str, body: &str) {
        let t = to_wide(title);
        let b = to_wide(body);
        unsafe {
            MessageBoxW(
                Some(HWND::default()),
                PCWSTR(b.as_ptr()),
                PCWSTR(t.as_ptr()),
                MB_OK | MB_ICONINFORMATION,
            );
        }
    }

    pub fn msg_error(title: &str, body: &str) {
        let t = to_wide(title);
        let b = to_wide(body);
        unsafe {
            MessageBoxW(
                Some(HWND::default()),
                PCWSTR(b.as_ptr()),
                PCWSTR(t.as_ptr()),
                MB_OK | MB_ICONERROR,
            );
        }
    }

    fn known_folder(id: &windows_core::GUID) -> Option<PathBuf> {
        unsafe {
            if let Ok(pwstr) = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) {
                let s = pwstr.to_string().ok();
                CoTaskMemFree(Some(pwstr.0 as *const _));
                return s.filter(|p| !p.trim().is_empty()).map(PathBuf::from);
            }
        }
        None
    }

    pub fn documents_projects_dir() -> PathBuf {
        known_folder(&FOLDERID_Documents)
            .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("Documents")))
            .unwrap_or_else(|| PathBuf::from("."))
            .join(APP_NAME)
    }

    fn create_shortcut(
        lnk_path: &Path,
        target: &Path,
        working_dir: &Path,
        icon_path: &Path,
        description: &str,
    ) -> Result<(), String> {
        if let Some(parent) = lnk_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let res = (|| -> Result<(), String> {
                let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                    .map_err(|e| format!("CoCreateInstance(ShellLink) failed: {e}"))?;
                let target_w = to_wide(&target.to_string_lossy());
                let work_w = to_wide(&working_dir.to_string_lossy());
                let icon_w = to_wide(&icon_path.to_string_lossy());
                let desc_w = to_wide(description);
                let lnk_w = to_wide(&lnk_path.to_string_lossy());

                link.SetPath(PCWSTR(target_w.as_ptr()))
                    .map_err(|e| format!("SetPath failed: {e}"))?;
                link.SetWorkingDirectory(PCWSTR(work_w.as_ptr()))
                    .map_err(|e| format!("SetWorkingDirectory failed: {e}"))?;
                link.SetIconLocation(PCWSTR(icon_w.as_ptr()), 0)
                    .map_err(|e| format!("SetIconLocation failed: {e}"))?;
                link.SetDescription(PCWSTR(desc_w.as_ptr()))
                    .map_err(|e| format!("SetDescription failed: {e}"))?;

                let persist: IPersistFile = link
                    .cast()
                    .map_err(|e| format!("IPersistFile cast failed: {e}"))?;
                persist
                    .Save(PCWSTR(lnk_w.as_ptr()), true)
                    .map_err(|e| format!("Shortcut Save failed: {e}"))?;
                Ok(())
            })();
            CoUninitialize();
            res
        }
    }

    fn shortcut_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Some(programs) = known_folder(&FOLDERID_Programs) {
            paths.push(programs.join("StudyNodes.lnk"));
        }
        if let Some(desktop) = known_folder(&FOLDERID_Desktop) {
            paths.push(desktop.join("StudyNodes.lnk"));
        }
        paths
    }

    pub fn install_shortcuts(install_dir: &Path, projects_dir: &Path) -> Result<(), String> {
        let target = install_dir.join(APP_EXE_NAME);
        let icon = install_dir.join(ICON_FILE_NAME);
        for lnk in shortcut_paths() {
            create_shortcut(
                &lnk,
                &target,
                projects_dir,
                &icon,
                "StudyNodes - Visual Knowledge Canvas",
            )?;
        }
        Ok(())
    }

    pub fn remove_shortcuts() {
        for lnk in shortcut_paths() {
            if lnk.exists() {
                let _ = std::fs::remove_file(lnk);
            }
        }
    }

    fn set_reg_sz(hkey: HKEY, name: &str, value: &str) {
        let n = to_wide(name);
        let v = to_wide(value);
        let bytes = unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 2) };
        unsafe {
            let _ = RegSetValueExW(hkey, PCWSTR(n.as_ptr()), Some(0), REG_SZ, Some(bytes));
        }
    }

    fn set_reg_dword(hkey: HKEY, name: &str, value: u32) {
        let n = to_wide(name);
        let bytes = value.to_le_bytes();
        unsafe {
            let _ = RegSetValueExW(hkey, PCWSTR(n.as_ptr()), Some(0), REG_DWORD, Some(&bytes));
        }
    }

    pub fn register_uninstaller(install_dir: &Path, estimated_kb: u32) -> Result<(), String> {
        let subkey = to_wide(UNINSTALL_REG_KEY);
        let mut hkey = HKEY::default();
        let status = unsafe {
            RegCreateKeyW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                &mut hkey,
            )
        };
        if status.is_err() {
            return Err(format!("Failed registering uninstaller key: {status:?}"));
        }

        let setup_exe = install_dir.join(SETUP_EXE_NAME);
        let icon_path = install_dir.join(ICON_FILE_NAME);
        set_reg_sz(hkey, "DisplayName", APP_NAME);
        set_reg_sz(hkey, "DisplayVersion", APP_VERSION);
        set_reg_sz(hkey, "Publisher", APP_PUBLISHER);
        set_reg_sz(hkey, "InstallLocation", &install_dir.to_string_lossy());
        set_reg_sz(hkey, "DisplayIcon", &icon_path.to_string_lossy());
        set_reg_sz(
            hkey,
            "UninstallString",
            &format!("\"{}\" --uninstall", setup_exe.display()),
        );
        set_reg_sz(
            hkey,
            "QuietUninstallString",
            &format!("\"{}\" --uninstall --silent", setup_exe.display()),
        );
        set_reg_sz(hkey, "URLInfoAbout", "https://github.com/inziz/studynode");
        set_reg_dword(hkey, "EstimatedSize", estimated_kb);
        set_reg_dword(hkey, "NoModify", 1);
        set_reg_dword(hkey, "NoRepair", 1);

        unsafe {
            let _ = RegCloseKey(hkey);
        }
        Ok(())
    }

    pub fn unregister_uninstaller() {
        let subkey = to_wide(UNINSTALL_REG_KEY);
        unsafe {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(subkey.as_ptr()));
        }
    }

    pub fn schedule_self_cleanup(install_dir: &Path) {
        if install_dir.exists() {
            let dir_str = install_dir.to_string_lossy().replace('"', "");
            let _ = std::process::Command::new("cmd")
                .args([
                    "/C",
                    &format!("ping 127.0.0.1 -n 2 > NUL & rmdir /S /Q \"{dir_str}\""),
                ])
                .spawn();
        }
    }
}

fn run_install(cfg: &SetupConfig) -> Result<(), String> {
    let install_dir = cfg
        .install_dir_override
        .clone()
        .unwrap_or_else(default_install_dir);

    #[cfg(target_os = "windows")]
    let projects_dir = win::documents_projects_dir();
    #[cfg(not(target_os = "windows"))]
    let projects_dir = std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join("Documents").join(APP_NAME))
        .unwrap_or_else(|| PathBuf::from(".").join(APP_NAME));

    #[cfg(target_os = "windows")]
    if !cfg.silent {
        let prompt = format!(
            "Install {APP_NAME} v{APP_VERSION}?\n\n\
             Application folder:\n  {}\n\n\
             Projects folder:\n  {}\n\n\
             This will add Start Menu and Desktop shortcuts and register {APP_NAME} in Installed Apps.",
            install_dir.display(),
            projects_dir.display()
        );
        if !win::msg_confirm(&format!("{APP_NAME} Setup"), &prompt) {
            return Ok(());
        }
    }

    let app_bytes = load_app_binary()?;
    let est_kb = ((app_bytes.len() as u64) / 1024).max(1) as u32;
    let cur_setup = std::env::current_exe().ok();
    let app_exe = stage_install_files(&install_dir, &projects_dir, &app_bytes, cur_setup.as_deref())?;

    #[cfg(target_os = "windows")]
    {
        win::install_shortcuts(&install_dir, &projects_dir)?;
        win::register_uninstaller(&install_dir, est_kb)?;
        if !cfg.silent {
            if !cfg.no_launch
                && win::msg_confirm(
                    &format!("{APP_NAME} Installed"),
                    &format!(
                        "{APP_NAME} v{APP_VERSION} was installed successfully!\n\n\
                         Your study projects will be saved in:\n  {}\n\n\
                         Launch {APP_NAME} now?",
                        projects_dir.display()
                    ),
                )
            {
                let _ = std::process::Command::new(&app_exe)
                    .current_dir(&projects_dir)
                    .spawn();
            }
        } else if !cfg.no_launch {
            let _ = std::process::Command::new(&app_exe)
                .current_dir(&projects_dir)
                .spawn();
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (est_kb, app_exe);
    }

    Ok(())
}

fn run_uninstall(cfg: &SetupConfig) -> Result<(), String> {
    let install_dir = cfg
        .install_dir_override
        .clone()
        .unwrap_or_else(default_install_dir);

    #[cfg(target_os = "windows")]
    if !cfg.silent {
        let prompt = format!(
            "Uninstall {APP_NAME} from:\n  {}\n\n\
             Your study projects in Documents\\StudyNodes will NOT be deleted.",
            install_dir.display()
        );
        if !win::msg_confirm(&format!("Uninstall {APP_NAME}"), &prompt) {
            return Ok(());
        }
    }

    #[cfg(target_os = "windows")]
    {
        win::remove_shortcuts();
        win::unregister_uninstaller();
    }

    remove_staged_files(&install_dir)?;

    #[cfg(target_os = "windows")]
    {
        win::schedule_self_cleanup(&install_dir);
        if !cfg.silent {
            win::msg_info(
                &format!("{APP_NAME} Uninstalled"),
                &format!(
                    "{APP_NAME} has been removed.\n\nYour study projects in Documents\\StudyNodes were kept safely."
                ),
            );
        }
    }

    Ok(())
}

fn main() {
    let cfg = parse_args(std::env::args().skip(1));
    let res = match cfg.action {
        Action::Install => run_install(&cfg),
        Action::Uninstall => run_uninstall(&cfg),
    };
    if let Err(err) = res {
        #[cfg(target_os = "windows")]
        if !cfg.silent {
            win::msg_error(&format!("{APP_NAME} Setup Error"), &err);
        }
        eprintln!("{err}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn generated_ico_has_valid_header_and_png_payload() {
        assert!(APP_ICO_BYTES.len() > 30);
        // ICONDIR: reserved=0, type=1, count=1
        assert_eq!(&APP_ICO_BYTES[0..6], &[0, 0, 1, 0, 1, 0]);
        // Offset to image data must be 22
        assert_eq!(
            u32::from_le_bytes([
                APP_ICO_BYTES[18],
                APP_ICO_BYTES[19],
                APP_ICO_BYTES[20],
                APP_ICO_BYTES[21],
            ]),
            22
        );
        // Embedded PNG magic right at offset 22
        assert_eq!(&APP_ICO_BYTES[22..30], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn cli_flag_parsing_handles_install_uninstall_silent_and_dir() {
        let def = parse_args(Vec::<&str>::new());
        assert_eq!(def.action, Action::Install);
        assert!(!def.silent);
        assert!(!def.no_launch);

        let silent_uninstall = parse_args(["/U", "/S", "--dir", r"C:\Custom\SN", "--no-launch"]);
        assert_eq!(silent_uninstall.action, Action::Uninstall);
        assert!(silent_uninstall.silent);
        assert!(silent_uninstall.no_launch);
        assert_eq!(
            silent_uninstall.install_dir_override,
            Some(PathBuf::from(r"C:\Custom\SN"))
        );
    }

    #[test]
    fn stage_install_and_remove_preserves_user_projects() {
        let tmp = TempDir::new().unwrap();
        let install_dir = tmp.path().join("install");
        let projects_dir = tmp.path().join("Documents").join("StudyNodes");
        let fake_app = b"MZ_FAKE_STUDYNODES_BINARY";

        let app_exe = stage_install_files(&install_dir, &projects_dir, fake_app, None).unwrap();
        assert!(app_exe.is_file());
        assert!(install_dir.join(ICON_FILE_NAME).is_file());
        assert!(projects_dir.is_dir());

        // Create a user project file in Documents\StudyNodes
        let user_project = projects_dir.join("MyCourse.studynodes");
        std::fs::create_dir_all(&user_project).unwrap();
        std::fs::write(user_project.join("graph.json"), "{}").unwrap();

        // Uninstall must remove installed files while keeping user projects untouched
        remove_staged_files(&install_dir).unwrap();
        assert!(!app_exe.exists());
        assert!(!install_dir.join(ICON_FILE_NAME).exists());
        assert!(user_project.join("graph.json").is_file());
    }
}
