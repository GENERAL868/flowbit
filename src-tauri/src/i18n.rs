//! The few texts the Rust side shows itself: tray menu, notifications, dialog titles.
//! The window has its own translations in ui/i18n.js.

/// "en" or "ar": the preference, or Windows' display language for "system"
pub fn language(pref: &str) -> &'static str {
    match pref {
        "ar" => "ar",
        "en" => "en",
        _ => system_language(),
    }
}

fn system_language() -> &'static str {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetUserDefaultUILanguage() -> u16;
        }
        // Primary language ID 0x01 is Arabic
        if unsafe { GetUserDefaultUILanguage() } & 0x3ff == 0x01 {
            return "ar";
        }
    }
    "en"
}

pub fn t(lang: &str, key: &str) -> &'static str {
    let ar = lang == "ar";
    match key {
        "tray.show" => {
            if ar {
                "إظهار Flowbit"
            } else {
                "Show Flowbit"
            }
        }
        "tray.pause" => {
            if ar {
                "إيقاف الكل مؤقتًا"
            } else {
                "Pause all"
            }
        }
        "tray.resume" => {
            if ar {
                "استئناف الكل"
            } else {
                "Resume all"
            }
        }
        "tray.quit" => {
            if ar {
                "إنهاء"
            } else {
                "Quit"
            }
        }
        "notify.done" => {
            if ar {
                "اكتمل التحميل"
            } else {
                "Download finished"
            }
        }
        "dialog.openTorrent" => {
            if ar {
                "فتح ملف تورنت"
            } else {
                "Open a .torrent file"
            }
        }
        "dialog.shareFolder" => {
            if ar {
                "اختر مجلدًا لمشاركته"
            } else {
                "Choose a folder to share"
            }
        }
        "dialog.shareFile" => {
            if ar {
                "اختر ملفًا لمشاركته"
            } else {
                "Choose a file to share"
            }
        }
        "dialog.saveTorrent" => {
            if ar {
                "حفظ ملف التورنت"
            } else {
                "Save .torrent file"
            }
        }
        "dialog.downloadDir" => {
            if ar {
                "اختر مكان التنزيلات"
            } else {
                "Choose where downloads go"
            }
        }
        "dialog.openSubtitle" => {
            if ar {
                "اختر ملف ترجمة"
            } else {
                "Choose a subtitle file"
            }
        }
        "dialog.saveSubtitle" => {
            if ar {
                "حفظ الترجمة"
            } else {
                "Save the translated subtitles"
            }
        }
        "filter.torrent" => {
            if ar {
                "ملفات تورنت"
            } else {
                "Torrent files"
            }
        }
        "filter.subtitles" => {
            if ar {
                "ملفات ترجمة"
            } else {
                "Subtitles"
            }
        }
        _ => "",
    }
}
