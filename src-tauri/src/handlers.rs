//! Registers the app as the handler for magnet links and .torrent files, for the
//! current user only (HKEY_CURRENT_USER\Software\Classes, no admin rights needed).

use std::path::Path;

use winreg::{enums::HKEY_CURRENT_USER, RegKey};

const TORRENT_PROG_ID: &str = "Flowbit.torrent";

pub fn register(exe: &Path, file_icon: Option<&Path>) -> anyhow::Result<()> {
    let classes = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags("Software\\Classes", winreg::enums::KEY_ALL_ACCESS)?;
    let exe = exe.display().to_string();
    let open_command = format!("\"{exe}\" \"%1\"");

    let (magnet, _) = classes.create_subkey("magnet")?;
    magnet.set_value("", &"URL:BitTorrent Magnet URL")?;
    magnet.set_value("URL Protocol", &"")?;
    magnet
        .create_subkey("DefaultIcon")?
        .0
        .set_value("", &format!("\"{exe}\",0"))?;
    magnet
        .create_subkey("shell\\open\\command")?
        .0
        .set_value("", &open_command)?;

    let (prog_id, _) = classes.create_subkey(TORRENT_PROG_ID)?;
    prog_id.set_value("", &"BitTorrent Document")?;
    let icon = file_icon
        .map(|p| format!("\"{}\"", p.display()))
        .unwrap_or_else(|| format!("\"{exe}\",0"));
    prog_id
        .create_subkey("DefaultIcon")?
        .0
        .set_value("", &icon)?;
    prog_id
        .create_subkey("shell\\open\\command")?
        .0
        .set_value("", &open_command)?;

    let (ext, _) = classes.create_subkey(".torrent")?;
    ext.set_value("", &TORRENT_PROG_ID)?;
    ext.set_value("Content Type", &"application/x-bittorrent")?;
    ext.create_subkey("OpenWithProgids")?
        .0
        .set_value(TORRENT_PROG_ID, &"")?;

    notify_shell();
    Ok(())
}

/// Whether magnet links and .torrent files open with this executable
pub fn is_registered(exe: &Path) -> bool {
    let classes = RegKey::predef(HKEY_CURRENT_USER);
    let exe = exe.display().to_string().to_lowercase();
    let points_here = |key: &str| {
        classes
            .open_subkey(key)
            .and_then(|k| k.get_value::<String, _>(""))
            .map(|cmd| cmd.to_lowercase().contains(&exe))
            .unwrap_or(false)
    };
    let ext_is_ours = classes
        .open_subkey("Software\\Classes\\.torrent")
        .and_then(|k| k.get_value::<String, _>(""))
        .map(|v| v == TORRENT_PROG_ID)
        .unwrap_or(false);
    points_here("Software\\Classes\\magnet\\shell\\open\\command") && ext_is_ours
}

fn notify_shell() {
    #[link(name = "shell32")]
    extern "system" {
        fn SHChangeNotify(event: i32, flags: u32, item1: *const u8, item2: *const u8);
    }
    const SHCNE_ASSOCCHANGED: i32 = 0x0800_0000;
    const SHCNF_IDLIST: u32 = 0;
    unsafe {
        SHChangeNotify(
            SHCNE_ASSOCCHANGED,
            SHCNF_IDLIST,
            std::ptr::null(),
            std::ptr::null(),
        )
    }
}
