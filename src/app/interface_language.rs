//! Reader interface language, independent of book language and document text.
//!
//! Vietnamese covers reading bar labels, the Screen panel, and this picker.
//! All other UI remains English until each surface is explicitly localized.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum InterfaceLanguage {
    Vietnamese,
    #[default]
    #[serde(other)]
    English,
}

impl InterfaceLanguage {
    pub(crate) fn is_english(&self) -> bool {
        *self == Self::English
    }

    /// Use only with Reader-owned interface literals, never book or user text.
    pub(super) fn text<'a>(self, english: &'a str) -> &'a str {
        if self != Self::Vietnamese {
            return english;
        }
        match english {
            "Interface language" => "Ngôn ngữ giao diện",
            "Documents" => "Tài liệu",
            "Pages" => "Trang",
            "Controls" => "Điều khiển",
            "Exit" => "Thoát",
            "Navigation" => "Điều hướng",
            "Appearance" => "Hiển thị",
            "Settings" => "Cài đặt",
            "Tools" => "Công cụ",
            "Search" => "Tìm kiếm",
            "Library" => "Thư viện",
            "Contents" => "Mục lục",
            "Bookmarks" => "Dấu trang",
            "Highlights" => "Đánh dấu",
            "Find" => "Tìm",
            "Go to" => "Đến trang",
            "View" => "Xem",
            "Text" => "Văn bản",
            "Scan" => "Quét",
            // Static section names and Screen-panel controls only. Never pass
            // a document title, font name, or other user-supplied value here.
            "Type" => "Kiểu chữ",
            "Layout" => "Bố cục",
            "Margins" => "Lề",
            "Style" => "Kiểu",
            "Screen" => "Màn hình",
            "Rendering" => "Kết xuất",
            "Spacing" => "Giãn cách",
            "Page" => "Trang",
            "Screen orientation" => "Hướng màn hình",
            "Landscape" => "Ngang",
            "Keep orientation" => "Giữ hướng màn hình",
            "Interface palette" => "Giao diện sáng tối",
            "Page colors" => "Màu trang",
            "Top status header (page turns)" => "Thanh trạng thái trên (lật trang)",
            "Top status header" => "Thanh trạng thái trên",
            "Default for new books" => "Mặc định cho sách mới",
            "Follow Launcher" => "Theo Launcher",
            "Upright" => "Dọc",
            "Upside down" => "Dọc ngược",
            "Landscape left" => "Ngang trái",
            "Landscape right" => "Ngang phải",
            "Left" => "Trái",
            "Right" => "Phải",
            "Off" => "Tắt",
            "On" => "Bật",
            "Light" => "Sáng",
            "Dark" => "Tối",
            "Original" => "Gốc",
            "Inverted" => "Đảo màu",
            _ => english,
        }
    }

    /// Return the canonical English search alias for a known Screen-panel row.
    /// This deliberately excludes user-supplied names and other menu surfaces.
    pub(super) fn screen_row_english<'a>(self, label: &'a str) -> &'a str {
        if self != Self::Vietnamese {
            return label;
        }
        match label {
            "Hướng màn hình" => "Screen orientation",
            "Ngang" => "Landscape",
            "Giữ hướng màn hình" => "Keep orientation",
            "Giao diện sáng tối" => "Interface palette",
            "Màu trang" => "Page colors",
            "Thanh trạng thái trên (lật trang)" => "Top status header (page turns)",
            "Thanh trạng thái trên" => "Top status header",
            "Mặc định cho sách mới" => "Default for new books",
            _ => label,
        }
    }
}
