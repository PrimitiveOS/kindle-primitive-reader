//! Reader interface language, independent of book language and document text.
//!
//! Vietnamese covers reading bars, top action titles, built-in bottom
//! configuration labels and the selection popup,
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

// Only fixed top Library actions belong here, never document names or paths.
const LIBRARY_LABELS: &[(&str, &str)] = &[
    ("Browse Documents", "Duyệt tài liệu"),
    ("Recently read", "Đã đọc gần đây"),
    ("Bookmark browser", "Duyệt dấu trang"),
    ("Open previous document", "Mở tài liệu trước"),
    ("Favorites", "Yêu thích"),
    (
        "Collections and reading lists",
        "Bộ sưu tập và danh sách đọc",
    ),
    ("Current book folder", "Thư mục sách hiện tại"),
    ("Previous file in folder", "Tệp trước trong thư mục"),
    ("Next file in folder", "Tệp tiếp trong thư mục"),
    ("Move to archive", "Chuyển vào lưu trữ"),
    ("Book details", "Chi tiết sách"),
];

// Fixed titles from the top Appearance, Settings and Tools groups. Dynamic
// detail strings may contain user-selected fonts or folders and stay untouched.
const TOP_CONTROL_LABELS: &[(&str, &str)] = &[
    ("Reading font", "Phông chữ đọc"),
    ("Reading profiles", "Hồ sơ đọc"),
    ("Reading guides", "Đường hướng dẫn đọc"),
    ("Typography rules", "Quy tắc kiểu chữ"),
    ("Switch to pages", "Chuyển sang lật trang"),
    ("Switch to continuous scroll", "Chuyển sang cuộn liên tục"),
    ("Larger text", "Chữ lớn hơn"),
    ("Smaller text", "Chữ nhỏ hơn"),
    ("Set font size", "Đặt cỡ chữ"),
    ("Wider margins", "Lề rộng hơn"),
    ("Narrower margins", "Lề hẹp hơn"),
    ("More line spacing", "Giãn dòng thêm"),
    ("Less line spacing", "Giãn dòng ít hơn"),
    ("Text alignment", "Căn chỉnh văn bản"),
    ("Hyphenation", "Ngắt từ"),
    ("Use regular body text", "Dùng nét chữ thường"),
    ("Use bold body text", "Dùng chữ đậm"),
    ("Use publisher fonts", "Dùng phông của nhà xuất bản"),
    ("Use selected reading font", "Dùng phông chữ đã chọn"),
    ("Reset text settings", "Đặt lại kiểu chữ"),
    ("Fit page", "Vừa trang"),
    ("Fit width", "Vừa chiều rộng"),
    ("Fit height", "Vừa chiều cao"),
    ("Rotate", "Xoay"),
    ("One page", "Một trang"),
    ("Two pages", "Hai trang"),
    ("Page turns", "Lật trang"),
    ("Scroll", "Cuộn"),
    ("Left to right", "Trái sang phải"),
    ("Right to left", "Phải sang trái"),
    ("Highlight style", "Kiểu tô sáng"),
    ("Quickstart guide", "Hướng dẫn bắt đầu"),
    ("Interface language", "Ngôn ngữ giao diện"),
    ("Bookmarks and notes", "Dấu trang và ghi chú"),
    ("Selection dialog position", "Vị trí hộp chọn"),
    ("Long-press on text", "Nhấn giữ văn bản"),
    ("Document defaults", "Mặc định tài liệu"),
    ("Reading gestures", "Cử chỉ đọc"),
    ("Navigation settings", "Cài đặt điều hướng"),
    ("Notifications", "Thông báo"),
    ("Links", "Liên kết"),
    ("Invert page-turn buttons", "Đảo nút lật trang"),
    ("At the end of a book", "Khi đọc hết sách"),
    ("Save reading data", "Lưu dữ liệu đọc"),
    ("Book metadata location", "Vị trí siêu dữ liệu sách"),
    ("Book metadata archive", "Kho siêu dữ liệu sách"),
    ("Style tweaks", "Điều chỉnh kiểu"),
    ("Font size rendering", "Cách hiển thị cỡ chữ"),
    ("Interface palette", "Giao diện sáng tối"),
    ("Screen orientation", "Hướng màn hình"),
    ("Refresh screen now", "Làm mới màn hình"),
    ("Reading status bar", "Thanh trạng thái đọc"),
    ("Front light −", "Đèn trước −"),
    ("Front light +", "Đèn trước +"),
    ("Warmth −", "Độ ấm −"),
    ("Warmth +", "Độ ấm +"),
    ("Night pages", "Trang ban đêm"),
    ("Reading toolbar size", "Cỡ thanh công cụ đọc"),
    ("Full screen refresh", "Làm mới toàn màn hình"),
    ("Menu activation", "Mở trình đơn"),
    ("Lock page touch", "Khóa cảm ứng trang"),
    ("Normal contrast", "Tương phản thường"),
    ("Higher contrast", "Tăng tương phản"),
    ("Crop", "Cắt lề"),
    ("Columns", "Cột"),
    ("Semi-auto crop", "Cắt lề bán tự động"),
    ("Recognize text", "Nhận dạng văn bản"),
    ("Read text", "Đọc văn bản"),
    ("Reflow scan", "Sắp xếp lại bản quét"),
    ("Read timer", "Hẹn giờ đọc"),
    ("Reading statistics", "Thống kê đọc"),
    ("Saved marks", "Dấu đã lưu"),
    ("Import KOReader position", "Nhập vị trí KOReader"),
    ("Reading data", "Dữ liệu đọc"),
    ("Book shortcuts", "Lối tắt sách"),
];

// Text → Style contains fixed labels and numeric settings only, never font names.
const TEXT_STYLE_LABELS: &[(&str, &str)] = &[
    ("Font weight", "Độ đậm chữ"),
    ("Font contrast", "Độ tương phản chữ"),
    ("Alignment", "Căn chỉnh"),
    ("Hyphenation", "Ngắt từ"),
    ("Custom rules", "Quy tắc tùy chỉnh"),
    ("Publisher", "Nhà xuất bản"),
    ("Start", "Đầu dòng"),
    ("Center", "Giữa"),
    ("Justify", "Căn đều"),
    ("Automatic", "Tự động"),
    ("Algorithmic", "Thuật toán"),
    ("Authored", "Theo tác giả"),
    ("Off", "Tắt"),
    ("On", "Bật"),
    ("Use publisher fonts", "Dùng phông của nhà xuất bản"),
    ("Use selected font", "Dùng phông đã chọn"),
    ("Reset text settings", "Đặt lại kiểu chữ"),
];

// Fixed-document View → Layout rows and choices contain no user-supplied labels.
const VIEW_LAYOUT_LABELS: &[(&str, &str)] = &[
    ("Reading mode", "Chế độ đọc"),
    ("Page gap", "Khoảng cách trang"),
    ("Page layout", "Bố cục trang"),
    ("Page overlap", "Phần trang chồng"),
    ("Overlap marker", "Dấu phần chồng"),
    ("Reading direction", "Hướng đọc"),
    ("Reading profiles", "Hồ sơ đọc"),
    ("Page turns", "Lật trang"),
    ("Scroll", "Cuộn"),
    ("One page", "Một trang"),
    ("Two pages", "Hai trang"),
    ("Off", "Tắt"),
    ("On", "Bật"),
    ("No indicator", "Không có dấu"),
    ("Gray out", "Tô xám"),
    ("Arrow", "Mũi tên"),
    ("Solid line", "Đường liền"),
    ("Dashed line", "Đường đứt"),
    ("Left to right", "Trái sang phải"),
    ("Right to left", "Phải sang trái"),
    ("Profiles", "Hồ sơ"),
];

// Reflow Text → Layout controls and choice labels are Reader-owned.
const TEXT_LAYOUT_LABELS: &[(&str, &str)] = &[
    ("Render mode", "Chế độ kết xuất"),
    ("New-book mode", "Chế độ cho sách mới"),
    ("Pages · single view here", "Trang · chỉ một trang ở đây"),
    ("Pages", "Trang"),
    ("One page", "Một trang"),
    ("Two pages", "Hai trang"),
    ("Line spacing", "Giãn dòng"),
    ("Set value", "Đặt giá trị"),
    ("Page overlap", "Phần trang chồng"),
    ("Overlap marker", "Dấu phần chồng"),
    ("On", "Bật"),
    ("Off", "Tắt"),
    ("No indicator", "Không có dấu"),
    ("Gray out", "Tô xám"),
    ("Arrow", "Mũi tên"),
    ("Solid line", "Đường liền"),
    ("Dashed line", "Đường đứt"),
    ("Less", "Ít hơn"),
    ("More", "Nhiều hơn"),
    ("Legacy", "Cũ"),
    ("Flat", "Phẳng"),
    ("Book", "Sách"),
];

// Fixed controls used by Type, Margins, Spacing, Rendering, View and Scan.
// These panels do not place document titles or user-selected font names in rows.
const OTHER_BOTTOM_LABELS: &[(&str, &str)] = &[
    ("Font size", "Cỡ chữ"),
    ("Smaller text", "Chữ nhỏ hơn"),
    ("Larger text", "Chữ lớn hơn"),
    ("More", "Thêm"),
    ("Reading mode", "Chế độ đọc"),
    ("Page turns", "Lật trang"),
    ("Scroll", "Cuộn"),
    ("Reading font", "Phông chữ đọc"),
    ("Text settings", "Cài đặt văn bản"),
    ("Profiles", "Hồ sơ"),
    ("Top and bottom", "Trên và dưới"),
    ("Independent", "Độc lập"),
    ("Linked", "Liên kết"),
    ("Reset margins", "Đặt lại lề"),
    ("Word spacing", "Khoảng cách từ"),
    ("Small", "Nhỏ"),
    ("Medium", "Vừa"),
    ("Large", "Lớn"),
    ("Letter spacing", "Khoảng cách chữ"),
    ("Tight", "Hẹp"),
    ("Normal", "Bình thường"),
    ("Wide", "Rộng"),
    ("Wider", "Rộng hơn"),
    ("Word expansion", "Mở rộng khoảng từ"),
    ("None", "Không"),
    ("Some", "Một ít"),
    ("Style tweaks", "Điều chỉnh kiểu"),
    ("Open", "Mở"),
    (
        "Base stylesheet (hold for default)",
        "Kiểu cơ sở (nhấn giữ để mặc định)",
    ),
    ("Auto", "Tự động"),
    ("Traditional", "Truyền thống"),
    ("Publisher CSS", "CSS nhà xuất bản"),
    ("Off", "Tắt"),
    ("On", "Bật"),
    ("Embedded fonts", "Phông chữ nhúng"),
    ("Details", "Chi tiết"),
    ("Zoom (dpi)", "Thu phóng (dpi)"),
    ("Custom zoom", "Thu phóng tùy chỉnh"),
    ("Set value", "Đặt giá trị"),
    ("Font kerning", "Khoảng cách cặp chữ"),
    ("Fast", "Nhanh"),
    ("Good", "Tốt"),
    ("Best", "Tốt nhất"),
    ("Font hinting", "Làm nét chữ"),
    ("Native", "Gốc"),
    ("Image scaling", "Thu phóng hình"),
    ("Smooth", "Mịn"),
    ("Night images", "Hình ban đêm"),
    ("Original", "Nguyên bản"),
    ("Inverted", "Đảo màu"),
    ("Fit", "Vừa khung"),
    ("Fit page", "Vừa trang"),
    ("Fit width", "Vừa chiều rộng"),
    ("Fit height", "Vừa chiều cao"),
    ("Zoom", "Thu phóng"),
    ("Columns", "Cột"),
    ("Rows", "Hàng"),
    ("Custom", "Tùy chỉnh"),
    ("Rotation", "Xoay"),
    ("Image dithering", "Hòa sắc hình"),
    ("Contrast", "Độ tương phản"),
    ("Fine contrast", "Tinh chỉnh tương phản"),
    ("Adaptive", "Thích ứng"),
    ("Normal contrast", "Tương phản thường"),
    ("Higher contrast", "Tăng tương phản"),
    ("Scan white level", "Mức trắng bản quét"),
    ("Low", "Thấp"),
    ("High", "Cao"),
    ("Auto straighten", "Tự làm thẳng"),
    ("Text selection", "Chọn văn bản"),
    ("Embedded first", "Ưu tiên văn bản nhúng"),
    ("Force OCR", "Luôn dùng OCR"),
    ("Decrease 1°", "Giảm 1°"),
    ("Increase 1°", "Tăng 1°"),
    ("Crop", "Cắt lề"),
    ("Semi-auto crop", "Cắt lề bán tự động"),
    ("Recognize text", "Nhận dạng văn bản"),
    ("Read text", "Đọc văn bản"),
    ("Reflow scan", "Sắp xếp lại bản quét"),
];

const OTHER_BOTTOM_PREFIXES: &[(&str, &str)] = &[
    ("Size · ", "Cỡ · "),
    ("Left · ", "Trái · "),
    ("Right · ", "Phải · "),
    ("Top · ", "Trên · "),
    ("Bottom · ", "Dưới · "),
    ("Legacy: ", "Cũ: "),
    ("Fine tune · ", "Tinh chỉnh · "),
    ("CJK width · ", "Độ rộng CJK · "),
    ("Fine · ", "Điều chỉnh · "),
    ("Limit · ", "Giới hạn · "),
    ("Margin · ", "Lề · "),
];

// Pages grid labels are fixed actions, never document or user content.
const PAGE_ACTION_LABELS: &[(&str, &str)] = &[
    ("Thumbnails", "Ảnh thu nhỏ"),
    ("Skim", "Đọc lướt"),
    ("Add bookmark", "Thêm dấu trang"),
    ("Links", "Liên kết"),
    ("History", "Lịch sử"),
    ("Previous location", "Vị trí trước"),
    ("Matches", "Kết quả tìm kiếm"),
];

// Fixed end-of-document action and preference labels only. The destructive
// confirmation uses its separate shared Sheet and is not localized here.
const END_DOCUMENT_LABELS: &[(&str, &str)] = &[
    ("End of document", "Hết tài liệu"),
    ("Mark as reading", "Đánh dấu đang đọc"),
    ("Mark as finished", "Đánh dấu đã đọc"),
    ("Book status", "Trạng thái sách"),
    ("Go to beginning", "Về đầu sách"),
    ("Open next file", "Mở tệp tiếp"),
    ("Delete file", "Xóa tệp"),
    ("Library", "Thư viện"),
    ("Close", "Đóng"),
    ("Always mark as finished", "Luôn đánh dấu đã đọc"),
    ("Show actions", "Hiện tùy chọn"),
    ("Stay on the last page", "Ở lại trang cuối"),
    ("Show book status", "Hiện trạng thái sách"),
    ("Return to Library", "Về thư viện"),
    (
        "Show book status, then Library",
        "Hiện trạng thái, rồi về thư viện",
    ),
    ("Ask to delete the file", "Hỏi trước khi xóa tệp"),
    ("On", "Bật"),
    ("Off", "Tắt"),
    ("Selected", "Đã chọn"),
    (
        "Selected · Prompts with this folder order",
        "Đã chọn · Sẽ hỏi với cách sắp xếp này",
    ),
];

// Search settings rows are Reader-owned text; numeric values stay verbatim.
const SEARCH_SETTINGS_LABELS: &[(&str, &str)] = &[
    ("Search settings", "Cài đặt tìm kiếm"),
    (
        "Show all results on text selection",
        "Hiện mọi kết quả khi chọn văn bản",
    ),
    ("Fit fixed pages during search", "Vừa trang cố định khi tìm"),
    ("Words in context", "Số từ ngữ cảnh"),
    ("Results per page", "Kết quả mỗi trang"),
    ("Max lines per result", "Số dòng tối đa mỗi kết quả"),
    (
        "Open the results list when you tap Find on selected text",
        "Mở danh sách kết quả khi tìm văn bản đã chọn",
    ),
    (
        "Temporarily show the whole page when finding the next or previous match",
        "Tạm hiện cả trang khi tìm kết quả tiếp hoặc trước",
    ),
    ("Flexible", "Linh hoạt"),
    ("Disabled", "Đã tắt"),
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

    /// Localize only a title from the fixed top Library action group.
    pub(super) fn library_title<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = LIBRARY_LABELS.iter().find(|(key, _)| *key == english) {
                return vietnamese;
            }
        }
        english
    }

    /// Preserve English queries for Library actions displayed in Vietnamese.
    pub(super) fn library_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = LIBRARY_LABELS.iter().find(|(_, value)| *value == label) {
                return english;
            }
        }
        label
    }

    /// Localize only an allowlisted built-in top control title.
    pub(super) fn top_control_title<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) =
                TOP_CONTROL_LABELS.iter().find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    /// Preserve English queries for the localized top controls.
    pub(super) fn top_control_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = TOP_CONTROL_LABELS.iter().find(|(_, value)| *value == label)
            {
                return english;
            }
        }
        label
    }

    /// Style rows only use app-owned labels and numeric fine-tune values.
    pub(super) fn text_style_title(self, english: &str) -> String {
        if self != Self::Vietnamese {
            return english.to_owned();
        }
        if let Some(value) = english.strip_prefix("Fine tune · ") {
            return format!("Tinh chỉnh · {value}");
        }
        TEXT_STYLE_LABELS
            .iter()
            .find_map(|(key, value)| (*key == english).then_some(*value))
            .unwrap_or(english)
            .to_owned()
    }

    /// Match the untranslated name while displaying a localized Style row.
    pub(super) fn text_style_english(self, label: &str) -> String {
        if self != Self::Vietnamese {
            return label.to_owned();
        }
        if let Some(value) = label.strip_prefix("Tinh chỉnh · ") {
            return format!("Fine tune · {value}");
        }
        TEXT_STYLE_LABELS
            .iter()
            .find_map(|(key, value)| (*value == label).then_some(*key))
            .unwrap_or(label)
            .to_owned()
    }

    /// Localize only fixed-document layout labels, preserving numeric values.
    pub(super) fn view_layout_title<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) =
                VIEW_LAYOUT_LABELS.iter().find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    /// Preserve English search aliases for localized layout rows.
    pub(super) fn view_layout_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = VIEW_LAYOUT_LABELS.iter().find(|(_, value)| *value == label)
            {
                return english;
            }
        }
        label
    }

    /// Translate fixed layout labels and known numeric prefixes only.
    pub(super) fn text_layout_title(self, english: &str) -> String {
        if self != Self::Vietnamese {
            return english.to_owned();
        }
        for (source, target) in [
            ("Side margins · ", "Lề bên · "),
            ("Fine · ", "Tinh chỉnh · "),
            ("Repeated lines · ", "Dòng lặp · "),
        ] {
            if let Some(value) = english.strip_prefix(source) {
                return format!("{target}{value}");
            }
        }
        TEXT_LAYOUT_LABELS
            .iter()
            .find_map(|(key, value)| (*key == english).then_some(*value))
            .unwrap_or(english)
            .to_owned()
    }

    /// Preserve English search aliases for layout rows, including numeric values.
    pub(super) fn text_layout_english(self, label: &str) -> String {
        if self != Self::Vietnamese {
            return label.to_owned();
        }
        for (source, target) in [
            ("Lề bên · ", "Side margins · "),
            ("Tinh chỉnh · ", "Fine · "),
            ("Dòng lặp · ", "Repeated lines · "),
        ] {
            if let Some(value) = label.strip_prefix(source) {
                return format!("{target}{value}");
            }
        }
        TEXT_LAYOUT_LABELS
            .iter()
            .find_map(|(key, value)| (*value == label).then_some(*key))
            .unwrap_or(label)
            .to_owned()
    }

    /// Translate labels on the six audited panels, retaining numeric suffixes.
    pub(super) fn other_bottom_title(self, english: &str) -> String {
        if self != Self::Vietnamese {
            return english.to_owned();
        }
        for &(source, target) in OTHER_BOTTOM_PREFIXES {
            if let Some(value) = english.strip_prefix(source) {
                return format!("{target}{value}");
            }
        }
        OTHER_BOTTOM_LABELS
            .iter()
            .find_map(|(key, value)| (*key == english).then_some(*value))
            .unwrap_or(english)
            .to_owned()
    }

    /// Return the English alias for a localized audited-panel row.
    pub(super) fn other_bottom_english(self, label: &str) -> String {
        if self != Self::Vietnamese {
            return label.to_owned();
        }
        for &(target, source) in OTHER_BOTTOM_PREFIXES {
            if let Some(value) = label.strip_prefix(source) {
                return format!("{target}{value}");
            }
        }
        OTHER_BOTTOM_LABELS
            .iter()
            .find_map(|(key, value)| (*value == label).then_some(*key))
            .unwrap_or(label)
            .to_owned()
    }

    /// Translate an app-owned Pages grid action title.
    pub(super) fn page_action_title<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) =
                PAGE_ACTION_LABELS.iter().find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    /// Preserve English menu searches for translated Pages actions.
    pub(super) fn page_action_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = PAGE_ACTION_LABELS.iter().find(|(_, value)| *value == label)
            {
                return english;
            }
        }
        label
    }

    /// Fixed selected-text popup labels only; never pass selected or user text.
    pub(super) fn selection_text<'a>(self, english: &'a str) -> &'a str {
        if self != Self::Vietnamese {
            return english;
        }
        match english {
            "Select" => "Chọn",
            "Highlight" => "Tô sáng",
            "Copy" => "Sao chép",
            "Note" => "Ghi chú",
            "Dictionary" => "Từ điển",
            "Wikipedia" => "Wikipedia",
            "Translate" => "Dịch",
            "Find" => "Tìm",
            "Chapter" => "Chương",
            "View HTML" => "Xem HTML",
            "Hyphenate" => "Ngắt từ",
            "Cancel" => "Hủy",
            "Done" => "Xong",
            "Highlight · choose style" => "Tô sáng · chọn kiểu",
            "Highlight · choose color" => "Tô sáng · chọn màu",
            "Lighten" => "Làm sáng",
            "Underline" => "Gạch dưới",
            "Strikethrough" => "Gạch ngang",
            "Invert" => "Đảo màu",
            "Red" => "Đỏ",
            "Orange" => "Cam",
            "Yellow" => "Vàng",
            "Green" => "Xanh lá",
            "Olive" => "Ô liu",
            "Cyan" => "Xanh lơ",
            "Blue" => "Xanh dương",
            "Purple" => "Tím",
            "Gray" => "Xám",
            "Tap a word to extend · Highlight to save · Copy text" => {
                "Chạm từ để mở rộng · Tô sáng để lưu · Sao chép"
            }
            "Tap another word to extend" => "Chạm từ khác để mở rộng",
            _ => english,
        }
    }

    /// Translate only fixed end-of-document UI labels and status text.
    pub(super) fn end_document_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) =
                END_DOCUMENT_LABELS.iter().find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    /// Preserve English searches for end-of-document preference rows.
    pub(super) fn end_document_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = END_DOCUMENT_LABELS
                .iter()
                .find(|(_, value)| *value == label)
            {
                return english;
            }
        }
        label
    }

    /// Fixed dictionary popup chrome only; never pass headwords, source names,
    /// definitions, lookup results, or dynamic notices through this function.
    pub(super) fn dictionary_popup_text<'a>(self, english: &'a str) -> &'a str {
        if self != Self::Vietnamese {
            return english;
        }
        match english {
            "Dictionary" => "Từ điển",
            "Save word" => "Lưu từ",
            "Remove word" => "Xóa từ",
            "Find" => "Tìm",
            "End find" => "Kết thúc tìm",
            "Wikipedia" => "Wikipedia",
            "Search book" => "Tìm trong sách",
            "Highlight" => "Tô sáng",
            "Unhighlight" => "Bỏ tô sáng",
            "Previous" => "Trước",
            "Next" => "Tiếp",
            "Expand" => "Mở rộng",
            "Collapse" => "Thu gọn",
            "Translate" => "Dịch",
            "Close" => "Đóng",
            "Cancel" => "Hủy",
            "Back" => "Quay lại",
            "Done" => "Xong",
            "Results" => "Kết quả",
            "Look up" => "Tra cứu",
            "Search with preset" => "Tìm bằng bộ cài sẵn",
            "Previous actions" => "Tùy chọn trước",
            "Next actions" => "Tùy chọn tiếp",
            "Sources" => "Nguồn",
            "All Results" => "Mọi kết quả",
            "Dictionary Results" => "Kết quả từ điển",
            "Dictionary Sources" => "Nguồn từ điển",
            "Headword only" => "Chỉ có từ mục",
            "Searching for similar words" => "Đang tìm từ tương tự",
            "No matches from this preset" => "Không có kết quả từ bộ này",
            "No dictionaries enabled for this book" => "Chưa bật từ điển cho sách này",
            "No matches in installed dictionaries" => "Không tìm thấy trong từ điển đã cài",
            "No results found" => "Không tìm thấy kết quả",
            "Word or phrase" => "Từ hoặc cụm từ",
            "Try similar spellings for this lookup" => "Thử cách viết tương tự",
            "Fuzzy search" => "Tìm gần đúng",
            "Edit lookup word" => "Sửa từ tra cứu",
            "Find in definition" => "Tìm trong định nghĩa",
            _ => english,
        }
    }

    /// Format a generated dictionary source count without changing its value.
    pub(super) fn dictionary_results_count(self, count: usize) -> String {
        if self == Self::Vietnamese {
            format!("{count} kết quả")
        } else {
            format!("{count} results")
        }
    }

    /// Fixed online lookup controls only; never pass article, query or provider text.
    pub(super) fn online_popup_text<'a>(self, english: &'a str) -> &'a str {
        if self != Self::Vietnamese {
            return english;
        }
        match english {
            "Cancel" => "Hủy",
            "Back" => "Quay lại",
            "Done" => "Xong",
            "Previous" => "Trước",
            "Next" => "Tiếp",
            "Next result" => "Kết quả tiếp",
            "Full article" => "Toàn bài",
            "Open image" => "Mở hình",
            "Edit topic" => "Sửa chủ đề",
            "From" => "Từ",
            "Language" => "Ngôn ngữ",
            "Looking up the text…" => "Đang tra cứu…",
            "Search" => "Tìm",
            "Translate" => "Dịch",
            "Wikipedia topic" => "Chủ đề Wikipedia",
            "Source language" => "Ngôn ngữ nguồn",
            "Lookup language" => "Ngôn ngữ tra cứu",
            "Apply" => "Áp dụng",
            "Enter a word or a short phrase." => "Nhập một từ hoặc cụm từ ngắn.",
            "Enter auto to detect the language, or a language code such as en, vi, fr or zh-CN." => {
                "Nhập auto để nhận diện ngôn ngữ, hoặc mã như en, vi, fr hay zh-CN."
            }
            "Enter a language code such as en, vi, fr or zh-CN." => {
                "Nhập mã ngôn ngữ như en, vi, fr hay zh-CN."
            }
            _ => english,
        }
    }

    /// Describe where the pending lookup sends data without changing its target.
    pub(super) fn online_disclosure(self, wikipedia: bool, target: &str) -> String {
        match (self, wikipedia) {
            (Self::Vietnamese, true) => {
                format!("Tìm kiếm gửi chủ đề này đến {target}.wikipedia.org.")
            }
            (Self::Vietnamese, false) => {
                format!("Dịch gửi văn bản này đến {target}. Mở Ngôn ngữ để thay đổi ngôn ngữ dịch.")
            }
            (_, true) => format!("Search sends this topic to {target}.wikipedia.org."),
            (_, false) => format!(
                "Translate sends this text to {target}. Open Language to change the translation language."
            ),
        }
    }

    /// Reader-owned Contents navigation labels, never chapter or query text.
    pub(super) fn contents_text<'a>(self, english: &'a str) -> &'a str {
        if self != Self::Vietnamese {
            return english;
        }
        match english {
            "Contents" => "Mục lục",
            "Alternative contents" => "Mục lục thay thế",
            "Search contents" => "Tìm mục lục",
            "Heading text, or * to show all" => "Nhập tiêu đề, hoặc * để hiện tất cả",
            "Find" => "Tìm",
            "Back" => "Quay lại",
            "No matching headings" => "Không tìm thấy tiêu đề phù hợp",
            "Building contents…" => "Đang tạo mục lục…",
            "Showing publisher contents" => "Đang hiện mục lục nhà xuất bản",
            "The alternative contents could not be built" => "Không thể tạo mục lục thay thế",
            "Generated outline" => "Mục lục tự tạo",
            "Build from headings in the book" => "Tạo từ các tiêu đề trong sách",
            "Manage custom contents" => "Quản lý mục lục tùy chỉnh",
            "Custom contents" => "Mục lục tùy chỉnh",
            "Current section" => "Mục hiện tại",
            "Page" => "Trang",
            "Chapter" => "Chương",
            "Previous" => "Trước",
            "Next" => "Tiếp",
            "Search" => "Tìm kiếm",
            "Clear" => "Xóa",
            "Query:" => "Từ tìm:",
            _ => english,
        }
    }

    /// Fixed bookmark-browser controls and mark-kind labels only.
    pub(super) fn bookmark_text<'a>(self, english: &'a str) -> &'a str {
        if self != Self::Vietnamese {
            return english;
        }
        match english {
            "Bookmark sources" => "Nguồn dấu trang",
            "Bookmark browser" => "Duyệt dấu trang",
            "Bookmark filters" => "Bộ lọc dấu trang",
            "Highlight color filter" => "Lọc màu tô sáng",
            "Find saved marks" => "Tìm dấu đã lưu",
            "Saved mark" => "Dấu đã lưu",
            "Choose books" => "Chọn sách",
            "Choose collections" => "Chọn bộ sưu tập",
            "Book information" => "Thông tin sách",
            "Choose an author" => "Chọn tác giả",
            "Choose a series" => "Chọn bộ sách",
            "Choose a language" => "Chọn ngôn ngữ",
            "Choose a keyword" => "Chọn từ khóa",
            "Book description" => "Mô tả sách",
            "Sources" => "Nguồn",
            "Filters" => "Bộ lọc",
            "History" => "Lịch sử",
            "Collections" => "Bộ sưu tập",
            "Selected books" => "Sách đã chọn",
            "Archived books" => "Sách đã lưu trữ",
            "Documents" => "Tài liệu",
            "Documents and subfolders" => "Tài liệu và thư mục con",
            "Choose a folder" => "Chọn thư mục",
            "Choose a folder with subfolders" => "Chọn thư mục và thư mục con",
            "Hold a source to make it the default" => "Nhấn giữ nguồn để đặt mặc định",
            "Default source" => "Nguồn mặc định",
            "Clear default source" => "Xóa nguồn mặc định",
            "Book" => "Sách",
            "Archived · Source file unavailable" => "Đã lưu trữ · Không có tệp nguồn",
            "Open book" => "Mở sách",
            "View cover" => "Xem bìa",
            "Read description" => "Đọc mô tả",
            "Source file" => "Tệp nguồn",
            "Book metadata" => "Siêu dữ liệu sách",
            "Reading metadata" => "Đang đọc siêu dữ liệu",
            "Not available" => "Không có sẵn",
            "Language" => "Ngôn ngữ",
            "Unknown language" => "Ngôn ngữ không rõ",
            "Series" => "Bộ sách",
            "Unknown series" => "Bộ sách không rõ",
            "Keywords" => "Từ khóa",
            "Keyword" => "Từ khóa",
            "Not supplied" => "Chưa cung cấp",
            "Description" => "Mô tả",
            "Apply book selection" => "Áp dụng sách đã chọn",
            "Select or clear all" => "Chọn hoặc bỏ chọn tất cả",
            "Select by author" => "Chọn theo tác giả",
            "Select by series" => "Chọn theo bộ sách",
            "Select by language" => "Chọn theo ngôn ngữ",
            "Select by keyword" => "Chọn theo từ khóa",
            "Unknown author" => "Tác giả không rõ",
            "Apply collections" => "Áp dụng bộ sưu tập",
            "Use this folder and subfolders" => "Dùng thư mục này và thư mục con",
            "Use this folder" => "Dùng thư mục này",
            "Up" => "Lên",
            "Clear filters" => "Xóa bộ lọc",
            "Highlight color" => "Màu tô sáng",
            "All colors" => "Mọi màu",
            "Choose one or more colors to export. Color and style filters are combined." => {
                "Chọn một hoặc nhiều màu để xuất. Bộ lọc màu và kiểu được kết hợp."
            }
            "Show saved highlights and notes with this color. Page bookmarks have no highlight color." => {
                "Hiện đoạn tô sáng và ghi chú có màu này. Dấu trang không có màu tô sáng."
            }
            "Reading book metadata" => "Đang đọc siêu dữ liệu sách",
            "No keywords" => "Không có từ khóa",
            "Books with matches" => "Sách có kết quả",
            "Filter by book" => "Lọc theo sách",
            "Sort direction" => "Thứ tự sắp xếp",
            "Reverse" => "Đảo ngược",
            "Forward" => "Xuôi",
            "Reading order" => "Thứ tự đọc",
            "Date added" => "Ngày thêm",
            "All types" => "Mọi loại",
            "All styles" => "Mọi kiểu",
            "Selected" => "Đã chọn",
            "Lighten" => "Làm sáng",
            "Underline" => "Gạch dưới",
            "Strikethrough" => "Gạch ngang",
            "Invert" => "Đảo màu",
            "Red" => "Đỏ",
            "Orange" => "Cam",
            "Yellow" => "Vàng",
            "Green" => "Xanh lá",
            "Olive" => "Ô liu",
            "Cyan" => "Xanh lơ",
            "Blue" => "Xanh dương",
            "Purple" => "Tím",
            "Gray" => "Xám",
            "Find" => "Tìm",
            "Find bookmarks" => "Tìm dấu trang",
            "Bookmark search results" => "Kết quả tìm dấu trang",
            "Bookmark, highlighted text, or note" => "Dấu trang, đoạn tô sáng hoặc ghi chú",
            "Case sensitive" => "Phân biệt hoa thường",
            "Bookmarks" => "Dấu trang",
            "Highlights" => "Đoạn tô sáng",
            "Notes" => "Ghi chú",
            "Back" => "Quay lại",
            "Bookmark, highlighted text or note" => "Dấu trang, đoạn tô sáng hoặc ghi chú",
            "Match case" => "Phân biệt hoa thường",
            "Page bookmark" => "Dấu trang",
            "Highlight" => "Đoạn tô sáng",
            "Note" => "Ghi chú",
            "First" => "Đầu",
            "Previous mark" => "Dấu trước",
            "Last" => "Cuối",
            "Next mark" => "Dấu tiếp",
            "View in book" => "Xem trong sách",
            "Archived mark" => "Dấu đã lưu trữ",
            "Previous text" => "Đoạn trước",
            "Next text" => "Đoạn tiếp",
            _ => english,
        }
    }

    pub(super) fn bookmark_counts(self, marks: usize, books: usize) -> String {
        if self == Self::Vietnamese {
            format!("{marks} dấu trong {books} sách")
        } else {
            let mark = if marks == 1 { "mark" } else { "marks" };
            let book = if books == 1 { "book" } else { "books" };
            format!("{marks} {mark} in {books} {book}")
        }
    }

    pub(super) fn bookmark_books_count(self, count: usize) -> String {
        if self == Self::Vietnamese {
            format!("{count} sách")
        } else {
            format!("{count} {}", if count == 1 { "book" } else { "books" })
        }
    }

    pub(super) fn bookmark_books_remaining(self, count: usize) -> String {
        if self == Self::Vietnamese {
            format!("Còn {count} sách")
        } else {
            format!("{count} books remaining")
        }
    }

    pub(super) fn bookmark_selected_count(self, count: usize) -> String {
        if self == Self::Vietnamese {
            format!("Đã chọn {count}")
        } else {
            format!("{count} selected")
        }
    }

    pub(super) fn bookmark_mark_count(self, count: usize) -> String {
        if self == Self::Vietnamese {
            format!("{count} dấu")
        } else {
            format!("{count} {}", if count == 1 { "mark" } else { "marks" })
        }
    }

    pub(super) fn bookmark_page(self, page: usize, total: usize) -> String {
        if self == Self::Vietnamese {
            format!("{page} trên {total}")
        } else {
            format!("{page} of {total}")
        }
    }

    /// Fixed document-search chrome; never pass a query or result excerpt here.
    pub(super) fn document_search_text<'a>(self, english: &'a str) -> &'a str {
        if self != Self::Vietnamese {
            return english;
        }
        match english {
            "Find in book" => "Tìm trong sách",
            "Search results" => "Kết quả tìm kiếm",
            "Chapter results" => "Kết quả trong chương",
            "Search text" => "Văn bản cần tìm",
            "Regular expression" => "Biểu thức chính quy",
            "Find all" => "Tìm tất cả",
            "Match case on" => "Phân biệt hoa thường: bật",
            "Match case off" => "Phân biệt hoa thường: tắt",
            "Ignore accents" => "Bỏ qua dấu",
            "Back" => "Quay lại",
            "Cancel" => "Hủy",
            "Cancel search" => "Hủy tìm kiếm",
            "Previous" => "Trước",
            "Next" => "Tiếp",
            "From start" => "Từ đầu",
            "From end" => "Từ cuối",
            "Results" => "Kết quả",
            "Search result options" => "Tùy chọn kết quả tìm kiếm",
            "All results" => "Tất cả kết quả",
            "Results in current chapter" => "Kết quả trong chương hiện tại",
            "Focus results near current page" => "Đến kết quả gần trang hiện tại",
            "Return to original page" => "Về trang ban đầu",
            "Done" => "Xong",
            "Search match" => "Kết quả tìm kiếm",
            _ => english,
        }
    }

    pub(super) fn matching_page(self, page: usize, total: usize) -> String {
        if self == Self::Vietnamese {
            format!("Trang khớp {page} trên {total}")
        } else {
            format!("Matching page {page} of {total}")
        }
    }

    pub(super) fn search_match_count(self, ordinal: usize, total: usize, found: bool) -> String {
        if self == Self::Vietnamese {
            format!(
                "Kết quả {ordinal} trên {total}{}",
                if found { " đã tìm" } else { "" }
            )
        } else {
            format!(
                "Match {ordinal} of {total}{}",
                if found { " found" } else { "" }
            )
        }
    }

    /// Translate only fixed Search settings labels and statuses.
    pub(super) fn search_settings_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = SEARCH_SETTINGS_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn search_settings_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = SEARCH_SETTINGS_LABELS
                .iter()
                .find(|(_, value)| *value == label)
            {
                return english;
            }
        }
        label
    }

    /// Fixed labels in the shared reading-settings numeric picker.
    pub(super) fn numeric_picker_text<'a>(self, english: &'a str) -> &'a str {
        if self != Self::Vietnamese {
            return english;
        }
        match english {
            "Rows per page" => "Số hàng mỗi trang",
            "Entry font size" => "Cỡ chữ mục",
            "Bookmark font size" => "Cỡ chữ dấu trang",
            "Max lines per bookmark" => "Số dòng tối đa mỗi dấu trang",
            "Words in context" => "Số từ ngữ cảnh",
            "Results per page" => "Kết quả mỗi trang",
            "Max lines per result" => "Số dòng tối đa mỗi kết quả",
            "Book author width" => "Độ rộng tên tác giả",
            "Book title width" => "Độ rộng tên sách",
            "Chapter title width" => "Độ rộng tên chương",
            "Disabled" => "Đã tắt",
            "Default" => "Mặc định",
            "Automatic" => "Tự động",
            "Disable" => "Tắt",
            "Close" => "Đóng",
            "Cancel" => "Hủy",
            "Apply" => "Áp dụng",
            _ => english,
        }
    }

    /// Render fixed numeric bounds while retaining the values and units.
    pub(super) fn numeric_picker_range(self, min: &str, max: &str, width: bool) -> String {
        let bounds = if self == Self::Vietnamese {
            format!("{min} đến {max}")
        } else {
            format!("{min} to {max}")
        };
        if width {
            if self == Self::Vietnamese {
                format!("{bounds} của chiều rộng màn hình")
            } else {
                format!("{bounds} of screen width")
            }
        } else {
            bounds
        }
    }

    /// Reader-owned library controls and prefixes, never filenames or queries.
    pub(super) fn library_view_text<'a>(self, english: &'a str) -> &'a str {
        if self != Self::Vietnamese {
            return english;
        }
        match english {
            "Find a document" => "Tìm tài liệu",
            "Folder order" => "Thứ tự thư mục",
            "Back" => "Quay lại",
            "Title, author or filename" => "Tên sách, tác giả hoặc tên tệp",
            "Natural name" => "Tên theo số",
            "Compare numbers by value" => "So sánh số theo giá trị",
            "File name" => "Tên tệp",
            "Compare names alphabetically" => "So sánh tên theo bảng chữ cái",
            "File type" => "Loại tệp",
            "Group files by extension" => "Nhóm tệp theo phần mở rộng",
            "Last read" => "Lần đọc cuối",
            "Unopened, then oldest reads · Browse only" => "Chưa mở, rồi cũ nhất · Chỉ khi duyệt",
            "Newest first · Browse only" => "Mới nhất trước · Chỉ khi duyệt",
            "Reading progress" => "Tiến độ đọc",
            "Finished first · Lowest active progress first" => {
                "Đã đọc xong trước · Tiến độ thấp trước"
            }
            "Highest progress first · Finished last" => "Tiến độ cao trước · Đã đọc xong cuối",
            "Selected" => "Đã chọn",
            "Reverse order" => "Đảo thứ tự",
            "On" => "Bật",
            "Off" => "Tắt",
            "Folder" => "Thư mục",
            "Protected · Open in Kindle Library" => "Được bảo vệ · Mở trong Thư viện Kindle",
            "Too large for Reader" => "Quá lớn đối với Reader",
            "Open in Kindle Library" => "Mở trong Thư viện Kindle",
            "Not supported in Reader" => "Reader không hỗ trợ",
            "Read this book" => "Đọc sách này",
            "EPUB · Extracted publication" => "EPUB · Sách đã giải nén",
            "Opened" => "Đã mở",
            "Unopened" => "Chưa mở",
            "In progress" => "Đang đọc",
            "On hold" => "Tạm dừng",
            "Finished" => "Đã đọc xong",
            "Exit" => "Thoát",
            "Documents" => "Tài liệu",
            "Search results" => "Kết quả tìm kiếm",
            "Clear" => "Xóa",
            "Up" => "Lên",
            "Find" => "Tìm",
            "Organize" => "Sắp xếp",
            "Controls" => "Điều khiển",
            "Reading data" => "Dữ liệu đọc",
            "Sort" => "Thứ tự",
            "Continue reading" => "Đọc tiếp",
            "This folder is empty." => "Thư mục này trống.",
            "No documents match your search." => "Không tìm thấy tài liệu phù hợp.",
            "Previous" => "Trước",
            "Next" => "Tiếp",
            "Showing part of this folder" => "Chỉ hiện một phần thư mục này",
            _ => english,
        }
    }

    /// Fixed controls in the main saved-highlight popup views only.
    pub(super) fn saved_highlight_text<'a>(self, english: &'a str) -> &'a str {
        if self != Self::Vietnamese {
            return english;
        }
        match english {
            "Overlapping highlights" => "Đoạn tô sáng chồng nhau",
            "Note" => "Ghi chú",
            "Highlight" => "Đoạn tô sáng",
            "Close" => "Đóng",
            "Previous" => "Trước",
            "Next" => "Tiếp",
            "Edit" => "Sửa",
            "Actions" => "Tùy chọn",
            "Edit note" => "Sửa ghi chú",
            "Add note" => "Thêm ghi chú",
            "Extend" => "Mở rộng",
            "Style" => "Kiểu",
            "Lighten" => "Làm sáng",
            "Underline" => "Gạch dưới",
            "Strikethrough" => "Gạch ngang",
            "Invert" => "Đảo màu",
            "Remove note" => "Xóa ghi chú",
            "Remove highlight" => "Xóa tô sáng",
            "Highlight style" => "Kiểu tô sáng",
            "New highlights in this book" => "Đoạn tô sáng mới trong sách",
            "Highlight color" => "Màu tô sáng",
            "Note marker" => "Dấu ghi chú",
            "Update all highlights?" => "Cập nhật mọi đoạn tô sáng?",
            "Remove note?" => "Xóa ghi chú?",
            "Remove highlight?" => "Xóa tô sáng?",
            "Gray highlight opacity" => "Độ đậm vùng tô xám",
            "Gray selection opacity" => "Độ đậm vùng chọn xám",
            "Highlight line height" => "Chiều cao vùng tô",
            "Color" => "Màu",
            "None" => "Không",
            "Side line" => "Đường bên",
            "Side mark" => "Dấu bên",
            "Red" => "Đỏ",
            "Orange" => "Cam",
            "Yellow" => "Vàng",
            "Green" => "Xanh lá",
            "Olive" => "Ô liu",
            "Cyan" => "Xanh lơ",
            "Blue" => "Xanh dương",
            "Purple" => "Tím",
            "Gray" => "Xám",
            "Selection color · On" => "Màu vùng chọn · Bật",
            "Selection color · Off" => "Màu vùng chọn · Tắt",
            "Apply to all highlights" => "Áp dụng cho mọi đoạn tô sáng",
            "Use reader default" => "Dùng mặc định Reader",
            "Set reader default" => "Đặt mặc định Reader",
            "Default" => "Mặc định",
            "Back" => "Quay lại",
            "Cancel" => "Hủy",
            "Apply" => "Áp dụng",
            "The highlighted passage will stay in your book." => {
                "Đoạn được tô sáng vẫn còn trong sách của bạn."
            }
            "The passage highlight and its note will be removed." => {
                "Phần tô sáng của đoạn văn và ghi chú của nó sẽ bị xóa."
            }
            _ => english,
        }
    }

    /// Keep the selected values/count and confirmation consequences explicit.
    pub(super) fn saved_highlight_apply_all(
        self,
        style: &str,
        color: &str,
        count: usize,
    ) -> String {
        if self == Self::Vietnamese {
            format!(
                "Áp dụng {} và {} cho tất cả {count} đoạn tô sáng? Ghi chú, vị trí và ngày tạo vẫn được giữ.",
                self.saved_highlight_text(style),
                self.saved_highlight_text(color)
            )
        } else {
            format!(
                "Apply {style} and {color} to all {count} highlights? Notes, positions and creation dates are kept."
            )
        }
    }

    /// Fixed footnote/link controls only; never pass document text or addresses.
    pub(super) fn navigation_popup_text<'a>(self, english: &'a str) -> &'a str {
        if self != Self::Vietnamese {
            return english;
        }
        match english {
            "Footnote" => "Chú thích",
            "Close" => "Đóng",
            "Open in book" => "Mở trong sách",
            "Look up" => "Tra cứu",
            "Scan link" => "Quét liên kết",
            "Back" => "Quay lại",
            "Link address" => "Địa chỉ liên kết",
            "Copy" => "Sao chép",
            "QR code" => "Mã QR",
            "Next" => "Tiếp",
            _ => english,
        }
    }

    /// Fixed Skim overlay labels, never chapter or publisher page labels.
    pub(super) fn skim_text<'a>(self, english: &'a str) -> &'a str {
        if self != Self::Vietnamese {
            return english;
        }
        match english {
            "Skim" => "Đọc lướt",
            "Page" => "Trang",
            "of" => "trên",
            "No open document" => "Chưa mở tài liệu",
            "Chapter back" => "Chương trước",
            "Mark back" => "Dấu trước",
            "Saved" => "Đã lưu",
            "Save page" => "Lưu trang",
            "Mark next" => "Dấu tiếp",
            "Chapter next" => "Chương tiếp",
            "Previous  ›" => "Trước  ›",
            "‹  Previous" => "‹  Trước",
            "Go to" => "Đến trang",
            "‹  Next" => "‹  Tiếp",
            "Next  ›" => "Tiếp  ›",
            "Cancel" => "Hủy",
            "Origin" => "Vị trí gốc",
            "Waiting…" => "Đang chờ…",
            "Read here" => "Đọc tại đây",
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
