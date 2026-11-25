//! Reader interface language, independent of book language and document text.
//!
//! Vietnamese covers reading bar labels, Navigation and Search actions, Screen settings,
//! and the language picker.
//! All other UI remains English until each surface is explicitly localized.

use serde::{Deserialize, Serialize};

// Names originate only from Reader's top Navigation group. The paired English
// strings are also search aliases; no title supplied by a document belongs here.
const NAVIGATION_LABELS: &[(&str, &str)] = &[
    ("Contents", "Mục lục"),
    ("Bookmarks", "Dấu trang"),
    ("Contents settings", "Cài đặt mục lục"),
    ("Remove bookmark", "Xóa dấu trang"),
    ("Add bookmark", "Thêm dấu trang"),
    ("Go to page", "Đến trang"),
    ("Previous chapter", "Chương trước"),
    ("Next chapter", "Chương tiếp"),
    ("Book map", "Sơ đồ sách"),
    ("Custom hidden flows", "Phần ẩn tùy chỉnh"),
    ("Page browser", "Xem các trang"),
    ("Skim", "Đọc lướt"),
    ("Chapter navigation", "Chuyển chương"),
    ("Auto turn", "Tự lật trang"),
    ("Hide non-linear fragments", "Ẩn phần không liên tục"),
    (
        "Default for non-linear fragments",
        "Mặc định cho phần không liên tục",
    ),
    ("Page numbering", "Đánh số trang"),
    ("Stable page number list", "Danh sách số trang cố định"),
    ("Stable pages in footer", "Trang cố định ở chân trang"),
    ("Printed pages in footer", "Trang in ở chân trang"),
    ("Stable pages in margin", "Trang cố định ở lề"),
    ("Printed pages in margin", "Trang in ở lề"),
    ("Links on this page", "Liên kết trên trang"),
    ("Previous location", "Vị trí trước"),
    ("Next location", "Vị trí tiếp"),
    ("Location history", "Lịch sử vị trí"),
    ("Footnote previews", "Xem trước chú thích"),
    ("Preview more links", "Xem trước thêm liên kết"),
];

// Titles in the Reader-owned Search group; values also supply reverse search aliases.
const SEARCH_LABELS: &[(&str, &str)] = &[
    ("Find in document", "Tìm trong tài liệu"),
    ("Search settings", "Cài đặt tìm kiếm"),
    ("Translate page", "Dịch trang"),
    ("Search results", "Kết quả tìm kiếm"),
    ("Search history", "Lịch sử tìm kiếm"),
    ("Find bookmarks", "Tìm dấu trang"),
    ("Menu search", "Tìm trong trình đơn"),
    ("Find a document", "Tìm tài liệu"),
    ("Dictionary lookup", "Tra từ điển"),
    ("Wikipedia", "Wikipedia"),
    ("Wikipedia settings", "Cài đặt Wikipedia"),
    ("Online history", "Lịch sử tra cứu trực tuyến"),
    ("Translation settings", "Cài đặt dịch"),
    ("Dictionary history", "Lịch sử từ điển"),
    ("Vocabulary", "Từ vựng"),
    ("Dictionaries", "Từ điển"),
];

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

    /// Localize only a title from the fixed top Navigation action group.
    pub(super) fn navigation_title<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = NAVIGATION_LABELS.iter().find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    /// Keep English search usable after displaying translated action titles.
    pub(super) fn navigation_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = NAVIGATION_LABELS.iter().find(|(_, value)| *value == label)
            {
                return english;
            }
        }
        label
    }

    /// These details originate in the same fixed action group, never in a book.
    pub(super) fn navigation_detail(self, english: &str) -> String {
        if self != Self::Vietnamese {
            return english.to_owned();
        }
        let fixed = match english {
            "Browse page thumbnails" => Some("Duyệt ảnh thu nhỏ của các trang"),
            "Preview a position, then keep it or return" => {
                Some("Xem trước vị trí, rồi giữ lại hoặc quay về")
            }
            "Choose Contents levels" => Some("Chọn cấp mục lục"),
            "On" => Some("Bật"),
            "Off" => Some("Tắt"),
            "Hidden" => Some("Đã ẩn"),
            "Shown" => Some("Đang hiện"),
            "Screen pages" => Some("Trang màn hình"),
            "Publisher labels" => Some("Số trang nhà xuất bản"),
            _ => None,
        };
        if let Some(translated) = fixed {
            return translated.to_owned();
        }
        let number_between = |prefix: &str, suffix: &str| {
            let number = english.strip_prefix(prefix)?.strip_suffix(suffix)?;
            (!number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()))
                .then_some(number)
        };
        if let Some(number) = number_between("Every ", " seconds") {
            return format!("Mỗi {number} giây");
        }
        if let Some(number) = number_between("", " characters per page") {
            return format!("{number} ký tự mỗi trang");
        }
        if let Some(number) = number_between("", " stable pages") {
            return format!("{number} trang cố định");
        }
        if let Some(number) = number_between("", " printed pages") {
            return format!("{number} trang in");
        }
        english.to_owned()
    }

    /// Localize only a title from the fixed top Search action group.
    pub(super) fn search_title<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = SEARCH_LABELS.iter().find(|(key, _)| *key == english) {
                return vietnamese;
            }
        }
        english
    }

    /// Preserve English queries when a Search action is displayed in Vietnamese.
    pub(super) fn search_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = SEARCH_LABELS.iter().find(|(_, value)| *value == label) {
                return english;
            }
        }
        label
    }

    /// Search group descriptions are fixed app text, never book or query content.
    pub(super) fn search_detail<'a>(self, english: &'a str) -> &'a str {
        if self != Self::Vietnamese {
            return english;
        }
        match english {
            "Find a reading control" => "Tìm điều khiển đọc",
            "Look up a topic" => "Tra cứu một chủ đề",
            "Display, sources and book preferences" => "Hiển thị, nguồn và tùy chọn cho sách",
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
