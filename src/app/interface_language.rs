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
    ("Previous saved mark", "Dấu đã lưu trước"),
    ("Next saved mark", "Dấu đã lưu tiếp"),
    ("Previous page bookmark", "Dấu trang trước"),
    ("Next page bookmark", "Dấu trang tiếp"),
    ("First saved mark", "Dấu đã lưu đầu tiên"),
    ("Last saved mark", "Dấu đã lưu cuối cùng"),
    ("Latest saved mark", "Dấu mới lưu nhất"),
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

// Fixed footer settings titles only; font names, custom text and dynamic details stay untouched.
const FOOTER_SETTINGS_TITLES: &[(&str, &str)] = &[
    ("Status bar presets", "Mẫu thanh trạng thái"),
    ("Item font", "Phông chữ của mục"),
    ("Status text", "Văn bản trạng thái"),
    ("Item font size", "Cỡ chữ của mục"),
    ("Container height", "Chiều cao vùng chứa"),
    ("Container bottom margin", "Lề dưới vùng chứa"),
    ("Bold text", "Chữ đậm"),
    ("Progress bar", "Thanh tiến độ"),
    ("Progress bar position", "Vị trí thanh tiến độ"),
    ("Progress bar thickness", "Độ dày thanh tiến độ"),
    (
        "Progress bar minimum width",
        "Độ rộng tối thiểu thanh tiến độ",
    ),
    (
        "Lock progress bar width at minimum",
        "Khóa độ rộng thanh tiến độ ở mức tối thiểu",
    ),
    ("Progress bar margins", "Lề thanh tiến độ"),
    ("Show initial position marker", "Hiện dấu vị trí ban đầu"),
    ("Status item separator", "Dấu phân cách mục trạng thái"),
    ("Show footer separator", "Hiện đường phân cách chân trang"),
    ("Status item symbols", "Ký hiệu mục trạng thái"),
    (
        "Pages left includes current page",
        "Số trang còn lại tính cả trang hiện tại",
    ),
    ("Progress percentage digits", "Số chữ số phần trăm tiến độ"),
    ("Footer alignment", "Căn chỉnh chân trang"),
    ("Top status header", "Thanh trạng thái trên"),
    (
        "About the top status header",
        "Giới thiệu thanh trạng thái trên",
    ),
    ("Refresh header automatically", "Tự làm mới thanh trên"),
    ("Header book title", "Tên sách trên thanh trên"),
    ("Header book author", "Tác giả trên thanh trên"),
    ("Header current time", "Giờ hiện tại trên thanh trên"),
    ("Header current page", "Trang hiện tại trên thanh trên"),
    ("Header total pages", "Tổng số trang trên thanh trên"),
    (
        "Header progress percentage",
        "Phần trăm tiến độ trên thanh trên",
    ),
    ("Header chapter marks", "Dấu chương trên thanh trên"),
    ("Header battery status", "Trạng thái pin trên thanh trên"),
    ("Header font size", "Cỡ chữ thanh trên"),
    ("Cycle custom items", "Lần lượt hiện mục tùy chỉnh"),
    ("Show items together", "Hiện các mục cùng nhau"),
    ("Overlap status bar", "Chồng lên thanh trạng thái"),
    ("Chapter markers", "Dấu chương"),
    ("Chapter marker thickness", "Độ dày dấu chương"),
    (
        "Lock status bar taps",
        "Khóa thao tác chạm thanh trạng thái",
    ),
    (
        "Hold status bar to skim",
        "Nhấn giữ thanh trạng thái để đọc lướt",
    ),
    ("Item maximum width", "Độ rộng tối đa của mục"),
    ("Visible status items", "Các mục trạng thái hiển thị"),
    ("Auto refresh items", "Tự làm mới các mục"),
    ("Hide inactive items", "Ẩn mục không hoạt động"),
    ("Hide battery when above", "Ẩn pin khi trên mức"),
    ("Custom text", "Văn bản tùy chỉnh"),
    ("Edit custom text", "Sửa văn bản tùy chỉnh"),
    ("Number of repetitions", "Số lần lặp"),
    ("Custom text tokens", "Ký hiệu văn bản tùy chỉnh"),
    ("Status item order", "Thứ tự mục trạng thái"),
    (
        "Time estimate in combined status",
        "Ước tính thời gian trong trạng thái kết hợp",
    ),
    ("12-hour clock", "Đồng hồ 12 giờ"),
];

// Fixed reading-font chooser/report controls; font filenames and family names stay untouched.
const READING_FONT_LABELS: &[(&str, &str)] = &[
    ("Device fonts", "Phông chữ thiết bị"),
    ("Add more fonts", "Thêm phông chữ"),
    (
        "Copy fonts to reader/data/fonts or a subfolder",
        "Sao chép phông chữ vào reader/data/fonts hoặc thư mục con",
    ),
    ("Preview unavailable", "Không thể xem trước"),
    ("Selected", "Đã chọn"),
    ("Embedded fonts", "Phông chữ nhúng"),
    (
        "Checking the book’s font families…",
        "Đang kiểm tra các họ phông của sách…",
    ),
    ("Usage in the whole book", "Mức sử dụng trong toàn bộ sách"),
    (
        "Usage checked in the current chapter",
        "Đã kiểm tra sử dụng trong chương hiện tại",
    ),
    (
        "Embedded fonts are off for this layout",
        "Phông chữ nhúng đang tắt cho bố cục này",
    ),
    (
        "No supported bundled fonts found",
        "Không tìm thấy phông đi kèm được hỗ trợ",
    ),
    ("Used", "Được dùng"),
    ("Not used", "Không được dùng"),
    ("Usage unchecked", "Chưa kiểm tra sử dụng"),
    ("Previous", "Trước"),
    ("Next", "Tiếp"),
    ("Done", "Xong"),
];

// Fixed footer settings values; user font names, custom text and numeric values stay untouched.
const FOOTER_SETTINGS_STATUSES: &[(&str, &str)] = &[
    ("On", "Bật"),
    ("Off", "Tắt"),
    ("Combined", "Kết hợp"),
    ("Page numbers", "Số trang"),
    ("Book progress", "Tiến độ sách"),
    ("Chapter and page", "Chương và trang"),
    ("Chapter pages left", "Trang còn lại trong chương"),
    ("Chapter time left", "Thời gian còn lại trong chương"),
    ("Book time left", "Thời gian còn lại trong sách"),
    ("Custom items", "Mục tùy chỉnh"),
    ("Minimal", "Tối giản"),
    ("Whole book", "Toàn bộ sách"),
    ("Current chapter", "Chương hiện tại"),
    ("Above status text", "Trên văn bản trạng thái"),
    ("Alongside status text", "Bên cạnh văn bản trạng thái"),
    ("Below status text", "Dưới văn bản trạng thái"),
    ("Thin", "Mỏng"),
    ("Medium", "Vừa"),
    ("Thick", "Dày"),
    ("Left", "Trái"),
    ("Center", "Giữa"),
    ("Right", "Phải"),
    ("Hyphen (-)", "Gạch nối (-)"),
    ("Vertical bar (|)", "Gạch đứng (|)"),
    ("Bullet (•)", "Dấu đầu dòng (•)"),
    ("Dot (·)", "Dấu chấm (·)"),
    ("No separator", "Không có dấu phân cách"),
    ("Icons", "Biểu tượng"),
    ("Letters", "Chữ cái"),
    ("Compact", "Gọn"),
    ("0 decimal places", "0 chữ số thập phân"),
    ("1 decimal place", "1 chữ số thập phân"),
    ("2 decimal places", "2 chữ số thập phân"),
    ("Chapter", "Chương"),
    ("Book", "Sách"),
    (
        "Tap the status bar to show one selected item at a time",
        "Chạm thanh trạng thái để lần lượt hiện từng mục đã chọn",
    ),
    (
        "Book author, book title, and chapter title",
        "Tác giả, tên sách và tên chương",
    ),
    (
        "On · When items share one line with no alongside track",
        "Bật · khi các mục chung một dòng và không có thanh bên cạnh",
    ),
    (
        "Off · Fill the space between arranged items",
        "Tắt · lấp khoảng trống giữa các mục",
    ),
    (
        "Show supported book, page, time and battery values",
        "Hiện giá trị sách, trang, thời gian và pin được hỗ trợ",
    ),
];

// Fixed status-bar controls and token explanations; user names and literal tokens stay untouched.
const STATUS_BAR_SETTINGS_LABELS: &[(&str, &str)] = &[
    ("Save current status bar", "Lưu thanh trạng thái hiện tại"),
    (
        "Save the current layout and progress bar options",
        "Lưu bố cục và tùy chọn thanh tiến độ hiện tại",
    ),
    ("Open preset actions", "Mở tùy chọn mẫu"),
    ("Apply to this Reader", "Áp dụng cho Reader này"),
    (
        "Update from current status bar",
        "Cập nhật từ thanh trạng thái hiện tại",
    ),
    ("Rename", "Đổi tên"),
    ("Delete", "Xóa"),
    ("Saved status bar preset", "Mẫu thanh trạng thái đã lưu"),
    ("Battery icon", "Biểu tượng pin"),
    ("Battery percentage", "Phần trăm pin"),
    ("On", "Bật"),
    ("Off", "Tắt"),
    ("Book title", "Tên sách"),
    ("Author", "Tác giả"),
    ("Series name and position", "Tên và vị trí trong bộ sách"),
    ("Total pages", "Tổng số trang"),
    ("Current page", "Trang hiện tại"),
    (
        "Pages left in the current chapter",
        "Số trang còn lại trong chương hiện tại",
    ),
    (
        "Book progress as a whole percentage",
        "Tiến độ sách theo phần trăm nguyên",
    ),
    (
        "Estimated time left in the book",
        "Thời gian đọc ước tính còn lại trong sách",
    ),
    ("Current chapter title", "Tên chương hiện tại"),
    (
        "Chapter progress as a whole percentage",
        "Tiến độ chương theo phần trăm nguyên",
    ),
    (
        "Estimated time left in the chapter",
        "Thời gian đọc ước tính còn lại trong chương",
    ),
    ("Full file path", "Đường dẫn tệp đầy đủ"),
    ("File name", "Tên tệp"),
    (
        "KOReader battery or charging symbol",
        "Biểu tượng pin hoặc sạc của KOReader",
    ),
    (
        "Selected status-item separator",
        "Dấu phân cách mục trạng thái đã chọn",
    ),
    (
        "Date in year-month-day format",
        "Ngày theo định dạng năm-tháng-ngày",
    ),
    ("Date in month-day format", "Ngày theo định dạng tháng-ngày"),
    (
        "Current time in hour:minute format",
        "Giờ hiện tại theo định dạng giờ:phút",
    ),
    ("Current time with seconds", "Giờ hiện tại có giây"),
    (
        "Two percent signs stay literal",
        "Hai dấu phần trăm được giữ nguyên",
    ),
    (
        "About the top status header",
        "Giới thiệu thanh trạng thái trên",
    ),
    (
        "For reflowable books, the header can appear above the footer in page-turn mode. It hides in continuous scroll.",
        "Với sách có thể dàn trang lại, thanh trên có thể hiện phía trên chân trang khi lật trang. Thanh này ẩn khi cuộn liên tục.",
    ),
    (
        "Swipe up from the bottom and open Screen. Turn on Top status header for this book or set Default for new books. Choose what it shows here.",
        "Vuốt lên từ dưới và mở Màn hình. Bật Thanh trạng thái trên cho sách này hoặc đặt mặc định cho sách mới. Chọn nội dung hiển thị tại đây.",
    ),
    ("Done", "Xong"),
    (
        "Default top status header",
        "Thanh trạng thái trên mặc định",
    ),
    (
        "Show the top status header when you open a new book in page-turn mode?",
        "Hiện thanh trạng thái trên khi mở sách mới ở chế độ lật trang?",
    ),
    (
        "Hide the top status header when you open a new book?",
        "Ẩn thanh trạng thái trên khi mở sách mới?",
    ),
    ("Set as default", "Đặt làm mặc định"),
    ("Cancel", "Hủy"),
];

// Fixed automatic page-turn settings; numeric values and reading behavior stay untouched.
const AUTO_TURN_LABELS: &[(&str, &str)] = &[
    ("Off", "Tắt"),
    ("Selected", "Đã chọn"),
    ("Custom interval", "Khoảng thời gian tùy chỉnh"),
    ("Enter 1 to 86,400 seconds", "Nhập từ 1 đến 86.400 giây"),
    ("Scroll distance", "Khoảng cách cuộn"),
    ("Fixed-page pan", "Dịch trang cố định"),
    ("Decrease by 50%", "Giảm 50%"),
    ("Decrease by 10%", "Giảm 10%"),
    ("Reset to 100%", "Đặt lại 100%"),
    ("Increase by 10%", "Tăng 10%"),
    ("Increase by 50%", "Tăng 50%"),
    ("Move 50 percentage points back", "Lùi 50 điểm phần trăm"),
    ("Move 10 percentage points back", "Lùi 10 điểm phần trăm"),
    ("One visible page at a time", "Mỗi lần một trang hiển thị"),
    (
        "Move 10 percentage points forward",
        "Tiến 10 điểm phần trăm",
    ),
    (
        "Move 50 percentage points forward",
        "Tiến 50 điểm phần trăm",
    ),
    ("Current distance", "Khoảng cách hiện tại"),
    (
        "Open a fixed-page document to change pan behavior",
        "Mở tài liệu trang cố định để đổi cách dịch trang",
    ),
    ("Horizontal overlap", "Chồng lấn ngang"),
    ("Vertical overlap", "Chồng lấn dọc"),
    ("Pan order", "Thứ tự dịch trang"),
    (
        "Vertical first · horizontal at the edge · tap to change",
        "Dọc trước · ngang ở mép · chạm để đổi",
    ),
    (
        "Horizontal first · vertical at the edge · tap to change",
        "Ngang trước · dọc ở mép · chạm để đổi",
    ),
    ("Vertical direction", "Hướng dọc"),
    (
        "Bottom to top · tap to reverse",
        "Dưới lên trên · chạm để đảo",
    ),
    (
        "Top to bottom · tap to reverse",
        "Trên xuống dưới · chạm để đảo",
    ),
    ("Horizontal reading direction", "Hướng đọc ngang"),
    (
        "Right to left · change in View settings",
        "Phải sang trái · đổi trong cài đặt Xem",
    ),
    (
        "Left to right · change in View settings",
        "Trái sang phải · đổi trong cài đặt Xem",
    ),
    ("Use document defaults", "Dùng mặc định của tài liệu"),
];

// Fixed selection settings labels; selected text and settings values stay untouched.
const SELECTION_SETTINGS_LABELS: &[(&str, &str)] = &[
    ("Top", "Trên"),
    ("Center", "Giữa"),
    ("Bottom", "Dưới"),
    ("By selected text", "Cạnh văn bản đã chọn"),
    ("Selected", "Đã chọn"),
    ("Dictionary on a single word", "Từ điển khi chọn một từ"),
    (
        "On · opens the local dictionary",
        "Bật · mở từ điển trên máy",
    ),
    (
        "Off · uses the default action",
        "Tắt · dùng thao tác mặc định",
    ),
    (
        "Auto-scroll when selection reaches a corner",
        "Tự cuộn khi vùng chọn đến góc",
    ),
    ("On", "Bật"),
    ("Off", "Tắt"),
    (
        "Highlight style and color prompt",
        "Hỏi kiểu và màu tô sáng",
    ),
    ("Very long press interval", "Thời gian nhấn rất lâu"),
    ("Ask with popup dialog", "Hỏi bằng cửa sổ nổi"),
    ("Do nothing", "Không làm gì"),
    ("Highlight", "Tô sáng"),
    ("Select and highlight", "Chọn và tô sáng"),
    ("Add note", "Thêm ghi chú"),
    ("Translate", "Dịch"),
    ("Wikipedia", "Wikipedia"),
    ("Dictionary", "Từ điển"),
    ("Full-text search", "Tìm toàn văn"),
    ("Reset to default", "Đặt lại mặc định"),
    ("Style", "Kiểu"),
    ("Color", "Màu"),
    ("Style and color", "Kiểu và màu"),
];

// Fixed Page Browser action labels; page and publisher content stay untouched.
const OVERVIEW_PAGE_ACTION_LABELS: &[(&str, &str)] = &[
    ("Remove page bookmark", "Xóa dấu trang này"),
    ("Bookmark page", "Đánh dấu trang"),
    ("Edit chapter", "Sửa chương"),
    ("Start chapter here", "Bắt đầu chương tại đây"),
    ("Mark flow boundary", "Đánh dấu ranh giới luồng"),
    ("Cancel", "Hủy"),
];

// Fixed saved-mark list settings and paging controls; mark text stays untouched.
const BOOKMARK_LIST_SETTINGS_LABELS: &[(&str, &str)] = &[
    ("Sort by page", "Sắp xếp theo trang"),
    ("Sort by date", "Sắp xếp theo ngày"),
    ("Document reading order", "Thứ tự đọc tài liệu"),
    (
        "Newest and oldest by saved time",
        "Mới nhất và cũ nhất theo thời điểm lưu",
    ),
    ("Selected", "Đã chọn"),
    ("Reverse sorting", "Đảo thứ tự sắp xếp"),
    ("On", "Bật"),
    ("Off", "Tắt"),
    ("Show in highlight rows", "Hiện trong hàng tô sáng"),
    ("Highlighted text", "Văn bản tô sáng"),
    ("Highlighted text and note", "Văn bản tô sáng và ghi chú"),
    (
        "Note if set, otherwise highlighted text",
        "Ghi chú nếu có, nếu không thì hiện văn bản tô sáng",
    ),
    ("Rows per page", "Số hàng mỗi trang"),
    ("Bookmark font size", "Cỡ chữ dấu trang"),
    ("Max lines per bookmark", "Số dòng tối đa mỗi dấu trang"),
    ("Disabled", "Đã tắt"),
    ("Flexible", "Linh hoạt"),
    (
        "Shrink bookmark font size to fit more text",
        "Thu nhỏ chữ dấu trang để hiện thêm văn bản",
    ),
    (
        "Unavailable with flexible rows",
        "Không khả dụng với hàng linh hoạt",
    ),
    (
        "Show separator between items",
        "Hiện đường phân cách giữa các mục",
    ),
    ("Show highlight colors", "Hiện màu tô sáng"),
    (
        "Also show default highlight color",
        "Cũng hiện màu tô sáng mặc định",
    ),
    (
        "Show highlight colors first",
        "Trước tiên hãy bật hiện màu tô sáng",
    ),
    ("Annotation exchange", "Trao đổi chú thích"),
    (
        "No saved marks in this book",
        "Không có dấu đã lưu trong sách này",
    ),
    ("Previous", "Trước"),
    ("Next", "Tiếp"),
    (
        "No matching bookmarks, highlights, or notes",
        "Không có dấu trang, đoạn tô sáng hoặc ghi chú phù hợp",
    ),
    ("No matching saved marks", "Không có dấu đã lưu phù hợp"),
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

// Built-in chapter navigation settings; never chapter titles or user text.
const CHAPTER_NAVIGATION_LABELS: &[(&str, &str)] = &[
    ("Chapter navigation", "Điều hướng chương"),
    ("Back", "Quay lại"),
    ("No previous chapter", "Không có chương trước"),
    ("No next chapter", "Không có chương tiếp"),
    ("No chapter levels", "Không có cấp chương"),
    (
        "The active Contents has no chapter destinations",
        "Mục lục hiện tại không có đích chương",
    ),
    (
        "Use levels for chapter jumps",
        "Dùng cấp mục lục để chuyển chương",
    ),
    (
        "Turn off a level to enable this option",
        "Tắt một cấp để bật tùy chọn này",
    ),
    (
        "Chapter jumps and indexed chapter progress use the included levels",
        "Chuyển chương và tiến độ theo mục lục dùng các cấp đã chọn",
    ),
    (
        "Chapter jumps and reading time use the included levels",
        "Chuyển chương và thời gian đọc dùng các cấp đã chọn",
    ),
    (
        "Chapter jumps, pages left, and reading time use the included levels",
        "Chuyển chương, số trang còn lại và thời gian đọc dùng các cấp đã chọn",
    ),
    (
        "Previous and next chapter use the included levels",
        "Chương trước và tiếp theo dùng các cấp đã chọn",
    ),
    ("Progress markers", "Dấu tiến độ"),
    (
        "The reading header, footer and Skim use the included levels",
        "Thanh trên, thanh dưới và Đọc lướt dùng các cấp đã chọn",
    ),
    (
        "The footer and Skim use the included levels",
        "Thanh dưới và Đọc lướt dùng các cấp đã chọn",
    ),
    (
        "The footer and Skim use included levels when ready",
        "Thanh dưới và Đọc lướt dùng các cấp đã chọn khi sẵn sàng",
    ),
    (
        "The reading header uses the included levels for this book",
        "Thanh trên dùng các cấp đã chọn cho sách này",
    ),
    (
        "Chapter titles from ticks only",
        "Tên chương chỉ từ dấu cấp đã chọn",
    ),
    (
        "Use included levels for the footer and new saved marks",
        "Dùng các cấp đã chọn cho thanh dưới và dấu lưu mới",
    ),
];

// Reader-owned Contents settings labels and statuses only.
const CONTENTS_SETTINGS_LABELS: &[(&str, &str)] = &[
    ("Rows per page", "Số hàng mỗi trang"),
    ("Entry font size", "Cỡ chữ mục"),
    ("Show chapter length", "Hiện độ dài chương"),
    ("Dot leaders", "Đường chấm dẫn"),
    ("Alternative table of contents", "Mục lục thay thế"),
    ("On", "Bật"),
    ("Off", "Tắt"),
    (
        "Disable custom contents to switch",
        "Tắt mục lục tùy chỉnh để chuyển",
    ),
    ("Document headings · On", "Tiêu đề tài liệu · Bật"),
    ("Publisher contents · Off", "Mục lục nhà xuất bản · Tắt"),
];

// Fixed page-numbering controls; publisher-provided labels are not passed here.
const NUMBERING_LABELS: &[(&str, &str)] = &[
    ("Page numbering", "Đánh số trang"),
    ("Page numbering defaults", "Mặc định đánh số trang"),
    ("Back", "Quay lại"),
    ("Numbering chapters", "Đang đánh số chương"),
    ("Publisher page numbers", "Số trang nhà xuất bản"),
    ("Screen page numbers", "Số trang màn hình"),
    (
        "Follow the current text layout",
        "Theo bố cục văn bản hiện tại",
    ),
    ("Character-count pages", "Trang theo số ký tự"),
    ("Characters per page", "Số ký tự mỗi trang"),
    (
        "Choose a value from 500 to 3,000",
        "Chọn giá trị từ 500 đến 3.000",
    ),
    ("Defaults for new books", "Mặc định cho sách mới"),
    ("Disable default character count", "Tắt số ký tự mặc định"),
    (
        "Override publisher page numbers",
        "Ghi đè số trang nhà xuất bản",
    ),
    (
        "Ask about publisher numbers",
        "Hỏi về số trang nhà xuất bản",
    ),
    ("Stable pages in footer", "Trang cố định ở chân trang"),
    ("Stable pages in margin", "Trang cố định ở lề"),
    ("Margin label size", "Cỡ nhãn ở lề"),
    ("On", "Bật"),
    ("Off", "Tắt"),
];

// Fixed reading-guide UI only; never pass book text or chapter names.
const READING_GUIDE_LABELS: &[(&str, &str)] = &[
    ("Reading guides", "Đường hướng dẫn đọc"),
    ("About reading guides", "Giới thiệu đường hướng dẫn đọc"),
    ("Back", "Quay lại"),
    ("Show reading guides", "Hiện đường hướng dẫn đọc"),
    ("Line thickness", "Độ dày đường"),
    ("Margin from the edges", "Khoảng cách từ mép"),
    ("Line intensity", "Độ đậm đường"),
    ("Move guides inward", "Di chuyển đường vào trong"),
    ("On", "Bật"),
    ("Off", "Tắt"),
    ("Two vertical lines", "Hai đường dọc"),
    (
        "Use them to mark the area to focus on while reading.",
        "Dùng chúng để đánh dấu vùng cần tập trung khi đọc.",
    ),
    ("Moving guides", "Di chuyển đường hướng dẫn"),
    (
        "The lines can move inward as you turn pages. Set the interval to 0 to keep them still.",
        "Các đường có thể dịch vào trong khi lật trang. Đặt khoảng cách lật trang là 0 để giữ nguyên.",
    ),
    ("Enter 1 to 20 pixels", "Nhập từ 1 đến 20 pixel"),
    (
        "Enter 1.0 to 45.0 percent",
        "Nhập từ 1.0 đến 45.0 phần trăm",
    ),
    (
        "Enter 1 to 10, from light to dark",
        "Nhập từ 1 đến 10, từ nhạt đến đậm",
    ),
    (
        "Enter 1 to 9,999 pages, or 0 to keep the guides still",
        "Nhập từ 1 đến 9.999 trang, hoặc 0 để giữ đường cố định",
    ),
    ("Set", "Đặt"),
];

// Fixed bottom typography availability and Style tweaks controls; CSS and user data stay untouched.
const BOTTOM_TYPOGRAPHY_LABELS: &[(&str, &str)] = &[
    ("Open a reflowable EPUB", "Mở EPUB có thể dàn trang lại"),
    (
        "Requires a horizontal left-to-right EPUB",
        "Cần EPUB ngang từ trái sang phải",
    ),
    ("Requires one-page reading", "Cần chế độ đọc một trang"),
    ("Turn on Publisher CSS", "Bật CSS của nhà xuất bản"),
    ("No embedded fonts found", "Không tìm thấy phông chữ nhúng"),
    (
        "Open a reflowable book first",
        "Trước tiên hãy mở sách có thể dàn trang lại",
    ),
    ("Enable style tweaks", "Bật điều chỉnh kiểu"),
    ("On for this book", "Bật cho sách này"),
    (
        "Off; choices and CSS retained",
        "Tắt; giữ các lựa chọn và CSS",
    ),
    ("Edit book CSS", "Sửa CSS của sách"),
    (
        "Supported CSS only; review before applying",
        "Chỉ hỗ trợ CSS được cho phép; xem lại trước khi áp dụng",
    ),
    ("Import book CSS", "Nhập CSS cho sách"),
    (
        "Copy a .css file to reader/data/style-tweaks",
        "Sao chép tệp .css vào reader/data/style-tweaks",
    ),
    ("Book CSS", "CSS của sách"),
    ("On", "Bật"),
    ("Off; text retained", "Tắt; giữ văn bản"),
    ("Clear book CSS", "Xóa CSS của sách"),
    ("Saved settings preserved", "Đã giữ cài đặt đã lưu"),
    (
        "Unavailable tweaks retained",
        "Đã giữ các điều chỉnh không khả dụng",
    ),
    ("Links", "Liên kết"),
    ("Spacing between paragraphs", "Khoảng cách giữa các đoạn"),
    ("Use book spacing", "Dùng khoảng cách của sách"),
    ("Full", "Đầy đủ"),
    ("Half", "Một nửa"),
    ("None", "Không"),
    ("Images", "Hình ảnh"),
    ("Tables", "Bảng"),
    ("Pseudo-elements", "Phần tử giả"),
    ("List items", "Mục danh sách"),
    ("Normal line height", "Chiều cao dòng thông thường"),
    ("In-page footnote size", "Cỡ chú thích trong trang"),
    ("More footnote options", "Tùy chọn chú thích khác"),
    ("Supported reflow styles", "Kiểu dàn trang được hỗ trợ"),
    (
        "Margins, hiding, floats, arbitrary selectors and vendor hints are not supported.",
        "Không hỗ trợ lề, ẩn nội dung, phần tử nổi, bộ chọn tùy ý và gợi ý của nhà cung cấp.",
    ),
    (
        "When notes appear in the page",
        "Khi chú thích xuất hiện trên trang",
    ),
    (
        "Horizontal reading with marked notes in the same chapter",
        "Đọc ngang với chú thích được đánh dấu trong cùng chương",
    ),
    ("Reader settings still apply", "Cài đặt Reader vẫn áp dụng"),
    (
        "Explicit alignment, font override and line spacing remain independent controls.",
        "Căn lề được chỉ định, ghi đè phông và giãn dòng vẫn là các điều khiển độc lập.",
    ),
    ("Use default", "Dùng mặc định"),
    ("Selected", "Đã chọn"),
    (
        "Inherit future all-books changes",
        "Kế thừa các thay đổi sau này cho mọi sách",
    ),
    ("Override the default", "Ghi đè mặc định"),
    ("Off for this book", "Tắt cho sách này"),
    ("Stop using on all books", "Ngừng dùng cho mọi sách"),
    ("Use on all books", "Dùng cho mọi sách"),
    (
        "Apply to eligible books that use the default",
        "Áp dụng cho sách đủ điều kiện đang dùng mặc định",
    ),
    (
        "Existing books inherit unless they override it",
        "Sách hiện có kế thừa trừ khi ghi đè",
    ),
    (
        "Review a book-specific copy; original file stays unchanged",
        "Xem lại bản sao riêng cho sách; tệp gốc giữ nguyên",
    ),
    ("No CSS files found", "Không tìm thấy tệp CSS"),
    (
        "Copy UTF-8 CSS files to reader/data/style-tweaks",
        "Sao chép tệp CSS UTF-8 vào reader/data/style-tweaks",
    ),
];

// Fixed top Settings descriptions only; user fonts, paths and dynamic notices stay untouched.
const TOP_SETTINGS_DETAIL_LABELS: &[(&str, &str)] = &[
    ("By date", "Theo ngày"),
    ("By page in reading order", "Theo trang trong thứ tự đọc"),
    (
        "Settings for new books; existing books stay unchanged",
        "Cài đặt cho sách mới; sách hiện có giữ nguyên",
    ),
    (
        "Assign taps, holds and swipes",
        "Gán thao tác chạm, nhấn giữ và vuốt",
    ),
    (
        "Touch controls, tap zones and reading direction",
        "Điều khiển cảm ứng, vùng chạm và hướng đọc",
    ),
    (
        "Back action, opening location and Skim position",
        "Thao tác quay lại, vị trí mở và vị trí đọc lướt",
    ),
    (
        "Link taps and return gesture",
        "Chạm liên kết và cử chỉ quay lại",
    ),
    (
        "Previous and next buttons are swapped",
        "Đã đổi chức năng nút trước và tiếp",
    ),
    (
        "Standard physical button directions",
        "Hướng nút vật lý tiêu chuẩn",
    ),
    (
        "Supported user CSS and live defaults",
        "CSS người dùng được hỗ trợ và mặc định hiện hành",
    ),
    ("Light text on dark pages", "Chữ sáng trên trang tối"),
    ("Dark text on light pages", "Chữ tối trên trang sáng"),
    ("Large labels and controls", "Nhãn và điều khiển lớn"),
    ("Compact labels and controls", "Nhãn và điều khiển gọn"),
    (
        "Physical page buttons, Unlock touch and Done stay available",
        "Nút lật trang vật lý, Mở khóa cảm ứng và Xong vẫn khả dụng",
    ),
];

// Fixed reading-profile controls; profile names and saved rules stay untouched.
const READING_PROFILE_LABELS: &[(&str, &str)] = &[
    ("Save current settings", "Lưu cài đặt hiện tại"),
    ("Reflowable books", "Sách có thể dàn trang lại"),
    ("Apply to this book", "Áp dụng cho sách này"),
    (
        "Replace with current settings",
        "Thay bằng cài đặt hiện tại",
    ),
    (
        "Open a reflowable book to use this profile",
        "Mở sách có thể dàn trang lại để dùng hồ sơ này",
    ),
    (
        "Open a matching document to use this profile",
        "Mở tài liệu phù hợp để dùng hồ sơ này",
    ),
    ("Included settings", "Cài đặt được bao gồm"),
    ("Apply on book opening", "Áp dụng khi mở sách"),
    ("Apply on book closing", "Áp dụng khi đóng sách"),
    ("Rename", "Đổi tên"),
    ("Duplicate", "Nhân bản"),
    ("Delete", "Xóa"),
    ("Select all", "Chọn tất cả"),
    ("Clear all", "Bỏ chọn tất cả"),
    ("Reading font", "Phông chữ đọc"),
    ("Font size", "Cỡ chữ"),
    ("Content zoom", "Độ phóng nội dung"),
    ("Margins", "Lề"),
    ("Line spacing", "Giãn dòng"),
    ("Word spacing", "Khoảng cách từ"),
    ("Word expansion", "Giãn từ"),
    ("Letter spacing", "Khoảng cách chữ"),
    ("CJK character width", "Chiều rộng ký tự CJK"),
    ("Page turns or scrolling", "Lật trang hoặc cuộn"),
    ("Facing pages", "Trang đối diện"),
    ("Alignment", "Căn lề"),
    ("Font weight", "Độ đậm chữ"),
    ("Font contrast", "Tương phản chữ"),
    ("Publisher styles", "Kiểu của nhà xuất bản"),
    ("Render mode", "Chế độ kết xuất"),
    ("Base stylesheet", "Bảng kiểu cơ sở"),
    ("Kerning", "Chỉnh khoảng cách cặp chữ"),
    ("Font hinting", "Hinting phông chữ"),
    ("Image scaling", "Thay đổi cỡ ảnh"),
    ("Night images", "Ảnh trong chế độ đêm"),
    ("Publisher fonts", "Phông của nhà xuất bản"),
    ("Hyphenation", "Ngắt từ"),
    ("Page overlap", "Chồng lấn trang"),
    (
        "Fit, zoom and reading mode",
        "Vừa trang, phóng và chế độ đọc",
    ),
    ("Page rotation", "Xoay trang"),
    ("Scan contrast", "Tương phản bản quét"),
    ("Scan white level", "Mức trắng bản quét"),
    ("Crop margin", "Lề cắt"),
    ("Auto straighten", "Tự căn thẳng"),
    ("Image dithering", "Hòa sắc ảnh"),
    ("Page gap", "Khoảng cách trang"),
    ("Reflow line spacing", "Giãn dòng khi dàn lại"),
    ("Zoom pan overlap", "Chồng lấn khi dịch lúc phóng"),
    ("Zoom pan direction", "Hướng dịch lúc phóng"),
    ("Reading direction", "Hướng đọc"),
    ("Cancel", "Hủy"),
    ("Replace this profile?", "Thay hồ sơ này?"),
    ("Replace", "Thay thế"),
    ("Delete this profile?", "Xóa hồ sơ này?"),
    ("Delete profile", "Xóa hồ sơ"),
    ("Always", "Luôn luôn"),
    ("When conditions match", "Khi điều kiện khớp"),
    ("Off", "Tắt"),
    ("Always apply", "Luôn áp dụng"),
    ("Apply when matched", "Áp dụng khi khớp"),
    ("Ask to execute", "Hỏi trước khi thực hiện"),
    ("Only within time window", "Chỉ trong khoảng thời gian"),
    ("Any time", "Bất kỳ lúc nào"),
    ("Execution delay", "Độ trễ thực hiện"),
    ("At opening", "Khi mở"),
    ("Closing conditions", "Điều kiện khi đóng"),
    (
        "Match any selected condition",
        "Khớp bất kỳ điều kiện đã chọn",
    ),
    ("Any book", "Bất kỳ sách nào"),
    ("Selected", "Đã chọn"),
    ("Closing screen orientation", "Hướng màn hình khi đóng"),
    ("Opening screen orientation", "Hướng màn hình khi mở"),
    ("Preferred opening orientation", "Hướng ưu tiên khi mở"),
    ("Book metadata contains", "Siêu dữ liệu sách chứa"),
    ("File path contains", "Đường dẫn tệp chứa"),
    ("Book is in a collection", "Sách nằm trong bộ sưu tập"),
    ("Only for new books", "Chỉ cho sách mới"),
    ("Clear conditions", "Xóa các điều kiện"),
    ("Follow Launcher", "Theo Launcher"),
    ("Upright", "Chiều dọc"),
    ("Upside down", "Đảo ngược"),
    ("Landscape left", "Ngang trái"),
    ("Landscape right", "Ngang phải"),
    ("Title", "Tên sách"),
    ("Authors", "Tác giả"),
    ("Series", "Bộ sách"),
    ("Series index", "Thứ tự trong bộ"),
    ("Language", "Ngôn ngữ"),
    ("Keywords", "Từ khóa"),
    ("Description", "Mô tả"),
    ("Unavailable", "Không khả dụng"),
    ("Start time", "Giờ bắt đầu"),
    ("End time", "Giờ kết thúc"),
    ("Unset", "Chưa đặt"),
    ("Clear time window", "Xóa khoảng thời gian"),
    ("Hour", "Giờ"),
    ("Minute", "Phút"),
    ("Set time", "Đặt giờ"),
    ("Apply reading profile?", "Áp dụng hồ sơ đọc?"),
    ("Apply", "Áp dụng"),
    ("Prompt", "Ngay khi mở"),
    ("Default", "Mặc định"),
    ("Apply during opening", "Áp dụng trong khi mở sách"),
    ("Apply just after opening", "Áp dụng ngay sau khi mở sách"),
    ("Custom", "Tùy chỉnh"),
    ("Custom delay", "Độ trễ tùy chỉnh"),
    ("Set delay", "Đặt độ trễ"),
    ("Left margin", "Lề trái"),
    ("Right margin", "Lề phải"),
    ("Top margin", "Lề trên"),
    ("Bottom margin", "Lề dưới"),
    ("Horizontal overlap", "Chồng lấn ngang"),
    ("Vertical overlap", "Chồng lấn dọc"),
    ("Excluded", "Đã loại trừ"),
    ("Link top and bottom", "Liên kết lề trên và dưới"),
    ("Linked", "Đã liên kết"),
    ("Independent", "Độc lập"),
    ("Page margins", "Lề trang"),
    ("Left, right, top and bottom", "Trái, phải, trên và dưới"),
    ("Font rendering", "Kết xuất chữ"),
    (
        "Weight, contrast, hinting and kerning",
        "Độ đậm, tương phản, hinting và khoảng cách cặp chữ",
    ),
    ("Book rendering", "Kết xuất sách"),
    (
        "Page mode, alignment, styles and images",
        "Chế độ trang, căn lề, kiểu và ảnh",
    ),
    ("Native", "Nguyên bản"),
    ("Auto", "Tự động"),
    ("Fast", "Nhanh"),
    ("Good", "Tốt"),
    ("Best", "Tốt nhất"),
    ("On", "Bật"),
    ("Smooth", "Mượt"),
    ("Original", "Gốc"),
    ("Inverted", "Đảo màu"),
    ("Page turns", "Lật trang"),
    ("Scrolling", "Cuộn"),
    ("One page", "Một trang"),
    ("Publisher", "Nhà xuất bản"),
    ("Start", "Đầu dòng"),
    ("Center", "Giữa"),
    ("Justify", "Căn đều"),
    ("None", "Không"),
    ("Traditional", "Truyền thống"),
];

// Fixed hidden-flow controls; stored positions and marker identities stay untouched.
const HIDDEN_FLOW_LABELS: &[(&str, &str)] = &[
    ("Custom hidden flows", "Luồng ẩn tùy chỉnh"),
    ("Edit in Page browser", "Sửa trong Duyệt trang"),
    ("On", "Bật"),
    ("Off", "Tắt"),
    ("Page browser", "Duyệt trang"),
    (
        "Hold a page to mark a flow boundary",
        "Nhấn giữ một trang để đánh dấu ranh giới luồng",
    ),
    ("Mark current page", "Đánh dấu trang hiện tại"),
    ("Clear inactive markers", "Xóa dấu không hoạt động"),
    ("Clear all markers", "Xóa mọi dấu"),
    ("Start hidden flow", "Bắt đầu luồng ẩn"),
    ("Restart regular flow", "Bắt đầu lại luồng thường"),
    ("Location unavailable", "Vị trí không khả dụng"),
    ("Clear inactive markers?", "Xóa dấu không hoạt động?"),
    (
        "Remove boundaries that currently have no effect. Other boundaries and your source book stay intact.",
        "Xóa các ranh giới hiện không có tác dụng. Các ranh giới khác và sách nguồn giữ nguyên.",
    ),
    ("Clear inactive", "Xóa dấu không hoạt động"),
    ("Clear custom hidden flows?", "Xóa luồng ẩn tùy chỉnh?"),
    (
        "Remove all custom flow boundaries for this book. You can create new boundaries from Page browser.",
        "Xóa mọi ranh giới luồng tùy chỉnh của sách này. Có thể tạo ranh giới mới trong Duyệt trang.",
    ),
    ("Clear markers", "Xóa dấu"),
    ("Flow boundary", "Ranh giới luồng"),
    (
        "Removing this marker joins the page to the preceding reading flow.",
        "Xóa dấu này sẽ nối trang vào luồng đọc trước đó.",
    ),
    ("Remove marker", "Xóa dấu"),
    (
        "Resume the regular reading flow from this page.",
        "Tiếp tục luồng đọc thường từ trang này.",
    ),
    (
        "Skip this page and following pages during regular reading. Links and Contents can still open them.",
        "Bỏ qua trang này và các trang sau khi đọc thường. Liên kết và Mục lục vẫn mở được các trang này.",
    ),
    ("Cancel", "Hủy"),
];

// Fixed book-archive controls; filenames and source/destination paths stay untouched.
const BOOK_ARCHIVE_LABELS: &[(&str, &str)] = &[
    ("Move current book", "Di chuyển sách hiện tại"),
    ("Archive folder", "Thư mục lưu trữ"),
    (
        "Choose a folder in Documents",
        "Chọn một thư mục trong Documents",
    ),
    ("Move this book?", "Di chuyển sách này?"),
    ("Move book", "Di chuyển sách"),
    ("Cancel", "Hủy"),
    ("From", "Từ"),
    ("To", "Đến"),
    ("Use this folder", "Dùng thư mục này"),
    ("Up", "Lên"),
];

// Fixed book-shortcut controls; user names, paths and identities stay untouched.
const BOOK_SHORTCUT_LABELS: &[(&str, &str)] = &[
    ("New shortcut", "Lối tắt mới"),
    ("Folder action", "Hành động với thư mục"),
    ("Browse folder", "Duyệt thư mục"),
    ("Open last-read book", "Mở sách đọc gần nhất"),
    ("Unavailable", "Không khả dụng"),
    ("Book", "Sách"),
    ("Folder", "Thư mục"),
    ("Add this folder", "Thêm thư mục này"),
    ("Up one folder", "Lên một thư mục"),
    ("Selected", "Đã chọn"),
    ("Include subfolders", "Bao gồm thư mục con"),
];

// Fixed footnote font editor text; font values and ranges stay untouched.
const FOOTNOTE_FONT_LABELS: &[(&str, &str)] = &[
    ("Relative", "Tương đối"),
    ("Fixed", "Cố định"),
    (
        "Offset from -10 to +5 pixels",
        "Độ lệch từ -10 đến +5 pixel",
    ),
    ("Fixed size from 12 to 255", "Cỡ cố định từ 12 đến 255"),
    (
        "Enter a whole-number offset from -10 to +5",
        "Nhập độ lệch nguyên từ -10 đến +5",
    ),
    (
        "Enter a whole number from 12 to 255",
        "Nhập số nguyên từ 12 đến 255",
    ),
    ("Set", "Đặt"),
];

// Fixed metadata-save/archive settings; archive paths and saved content stay untouched.
const METADATA_SETTINGS_LABELS: &[(&str, &str)] = &[
    ("On close or sleep", "Khi đóng sách hoặc ngủ"),
    ("Every 5 minutes", "Mỗi 5 phút"),
    ("Every 15 minutes", "Mỗi 15 phút"),
    ("Every 30 minutes", "Mỗi 30 phút"),
    ("Every 60 minutes", "Mỗi 60 phút"),
    ("After each change", "Sau mỗi thay đổi"),
    ("Selected", "Đã chọn"),
    ("About the archive", "Giới thiệu kho lưu trữ"),
    (
        "Save a separate copy of a book’s reading data",
        "Lưu một bản riêng dữ liệu đọc của sách",
    ),
    ("Archive folder", "Thư mục lưu trữ"),
    (
        "Choose a folder in Documents",
        "Chọn một thư mục trong Documents",
    ),
    (
        "Save metadata on book close",
        "Lưu siêu dữ liệu khi đóng sách",
    ),
    (
        "Save metadata before deleting",
        "Lưu siêu dữ liệu trước khi xóa",
    ),
    ("Archived books", "Sách đã lưu trữ"),
    (
        "View saved positions, bookmarks and highlights",
        "Xem vị trí, dấu trang và phần tô sáng đã lưu",
    ),
    ("On", "Bật"),
    ("Off", "Tắt"),
    ("Saved on close", "Đã lưu khi đóng sách"),
    ("Saved before deletion", "Đã lưu trước khi xóa"),
    ("Remove saved copy", "Xóa bản đã lưu"),
    (
        "Keep the book and its live reading data",
        "Giữ sách và dữ liệu đọc hiện tại",
    ),
    ("Bookmark", "Dấu trang"),
    (
        "The archive keeps settings, reading position, notes and highlights. Open a matching edition to recover its saved data. The original book stays where it is.",
        "Kho lưu cài đặt, vị trí đọc, ghi chú và phần tô sáng. Mở đúng phiên bản sách để khôi phục dữ liệu đã lưu. Sách gốc vẫn ở vị trí cũ.",
    ),
    ("Done", "Xong"),
    ("Restore book metadata?", "Khôi phục siêu dữ liệu sách?"),
    (
        "Restore progress, settings and marks.\nStart fresh removes the saved copy.",
        "Khôi phục tiến độ, cài đặt và đánh dấu.\nBắt đầu mới sẽ xóa bản đã lưu.",
    ),
    ("Restore", "Khôi phục"),
    ("Remove this saved copy?", "Xóa bản đã lưu này?"),
    (
        "The book and its current reading data stay in your Library.",
        "Sách và dữ liệu đọc hiện tại vẫn ở trong Thư viện.",
    ),
    ("Remove", "Xóa"),
    ("Move metadata archive?", "Di chuyển kho siêu dữ liệu?"),
    (
        "Merge saved reading data into this folder.\nKeep the newest dated copy of each edition.",
        "Hợp nhất dữ liệu đọc đã lưu vào thư mục này.\nGiữ bản có ngày mới nhất của từng phiên bản.",
    ),
    ("Move", "Di chuyển"),
    ("Start fresh", "Bắt đầu mới"),
    ("Cancel", "Hủy"),
];

// Fixed vocabulary-review controls; user words, book names and schedules stay untouched.
const VOCABULARY_SETTINGS_LABELS: &[(&str, &str)] = &[
    ("Review options", "Tùy chọn ôn tập"),
    ("Due for review", "Đến hạn ôn tập"),
    ("All scheduled words", "Mọi từ đã lên lịch"),
    ("Find a word", "Tìm một từ"),
    ("Clear search", "Xóa tìm kiếm"),
    ("Show due words only", "Chỉ hiện từ đến hạn"),
    (
        "Add dictionary lookups automatically",
        "Tự thêm từ tra trong từ điển",
    ),
    ("Save reading context", "Lưu ngữ cảnh đọc"),
    ("On", "Bật"),
    ("Off", "Tắt"),
    ("Filter by book", "Lọc theo sách"),
    (
        "Reset all review progress",
        "Đặt lại toàn bộ tiến độ ôn tập",
    ),
    ("Dictionary lookup", "Tra từ điển"),
    ("Current source", "Nguồn hiện tại"),
    ("Move this word here", "Chuyển từ này đến đây"),
    (
        "Excluded · Hold to rename",
        "Đã loại trừ · nhấn giữ để đổi tên",
    ),
    (
        "Included · Hold to rename",
        "Được bao gồm · nhấn giữ để đổi tên",
    ),
    ("New book title", "Tên sách mới"),
    ("Reading context", "Ngữ cảnh đọc"),
    ("Show definition", "Hiện định nghĩa"),
    ("Change source book", "Đổi sách nguồn"),
    ("Got it", "Đã nhớ"),
    ("Schedule the next review", "Lên lịch ôn tập tiếp theo"),
    ("Forgot", "Đã quên"),
    ("Review again in 5 minutes", "Ôn tập lại sau 5 phút"),
    ("Undo last review", "Hoàn tác lần ôn tập gần nhất"),
    ("Remove word", "Xóa từ"),
    ("Reset this word's progress", "Đặt lại tiến độ của từ này"),
    ("Change vocabulary book title", "Đổi tên sách từ vựng"),
    ("Move word to book", "Chuyển từ sang sách"),
    ("Book title", "Tên sách"),
    ("Save", "Lưu"),
    ("Word already saved", "Từ đã được lưu"),
    (
        "Keep reviewing, or replace its source and context with this lookup.",
        "Tiếp tục ôn tập, hoặc thay nguồn và ngữ cảnh bằng lần tra cứu này.",
    ),
    ("Replace", "Thay thế"),
    ("Cancel", "Hủy"),
    ("Reset review progress?", "Đặt lại tiến độ ôn tập?"),
    ("Remove this word?", "Xóa từ này?"),
    (
        "The word and reading context are kept. This word becomes due now.",
        "Giữ từ và ngữ cảnh đọc. Từ này đến hạn ôn ngay.",
    ),
    (
        "Words and reading context are kept. Every word becomes due now.",
        "Giữ các từ và ngữ cảnh đọc. Mọi từ đến hạn ôn ngay.",
    ),
    (
        "This removes the word, its context and its review history. You can save the word again.",
        "Xóa từ, ngữ cảnh và lịch sử ôn tập của từ. Bạn có thể lưu lại từ này.",
    ),
    ("Reset", "Đặt lại"),
    ("Remove", "Xóa"),
    ("Due now", "Đến hạn ngay"),
    ("Previous", "Trước"),
    ("Next", "Tiếp"),
];

// Fixed library highlight-export controls; user names and saved paths stay untouched.
const LIBRARY_EXPORT_LABELS: &[(&str, &str)] = &[
    ("History", "Lịch sử"),
    ("Choose books", "Chọn sách"),
    ("Collections", "Bộ sưu tập"),
    ("Use selected books", "Dùng sách đã chọn"),
    ("Select or clear all", "Chọn hoặc bỏ chọn tất cả"),
    ("Unknown author", "Không rõ tác giả"),
    ("Selected", "Đã chọn"),
    ("Use selected collections", "Dùng bộ sưu tập đã chọn"),
    ("Choose export format", "Chọn định dạng xuất"),
    ("Export settings", "Cài đặt xuất"),
    ("Highlight colors", "Màu tô sáng"),
    ("All colors", "Mọi màu"),
    ("All highlight styles", "Mọi kiểu tô sáng"),
    ("Kindle clippings", "Trích đoạn Kindle"),
    ("Requires all styles and colors", "Cần chọn mọi kiểu và màu"),
    ("Lighten", "Làm sáng"),
    ("Underline", "Gạch chân"),
    ("Strikethrough", "Gạch ngang"),
    ("Invert", "Đảo màu"),
    ("Export saved", "Đã lưu tệp xuất"),
    ("Plain text", "Văn bản thuần"),
    ("Kindle clippings text", "Văn bản trích đoạn Kindle"),
    ("Cancel export", "Hủy xuất"),
];

// Fixed backup-preview controls; component identities, counts and user data stay untouched.
const READING_BACKUP_LABELS: &[(&str, &str)] = &[
    ("Apply backup", "Áp dụng bản sao lưu"),
    (
        "Books and collections · merge",
        "Sách và bộ sưu tập · hợp nhất",
    ),
    ("Saved components", "Các thành phần đã lưu"),
    (
        "New files are selected. Tap an existing component to replace it.",
        "Các tệp mới đã được chọn. Chạm vào thành phần hiện có để thay thế thành phần đó.",
    ),
    (
        "Collections and custom navigation",
        "Bộ sưu tập và điều hướng tùy chỉnh",
    ),
    (
        "Selected · replace existing local data",
        "Đã chọn · thay thế dữ liệu hiện có trên máy",
    ),
    (
        "Skipped · keep existing local data",
        "Bỏ qua · giữ dữ liệu hiện có trên máy",
    ),
    (
        "Selected · add missing local data",
        "Đã chọn · thêm dữ liệu còn thiếu trên máy",
    ),
    (
        "Skipped · leave missing",
        "Bỏ qua · giữ nguyên trạng thái còn thiếu",
    ),
    (
        "Legacy library-only backup",
        "Bản sao lưu cũ chỉ có thư viện",
    ),
    (
        "This file contains no global controls, vocabulary or dated history.",
        "Tệp này không có cài đặt chung, từ vựng hoặc lịch sử có ngày.",
    ),
    (
        "Restore returns to the library tools",
        "Khôi phục đưa bạn về công cụ thư viện",
    ),
    (
        "Your documents and current network accounts are kept.",
        "Tài liệu và tài khoản mạng hiện tại của bạn được giữ lại.",
    ),
    ("Books to merge", "Sách cần hợp nhất"),
    ("saved components to apply", "thành phần đã lưu cần áp dụng"),
    ("New", "Mới"),
    ("already known", "đã biết"),
    (
        "existing positions and settings kept",
        "giữ vị trí và cài đặt hiện có",
    ),
    ("Bookmarks", "Dấu trang"),
    ("highlights", "phần tô sáng"),
    ("Notes", "Ghi chú"),
    ("duplicates", "bản trùng"),
    ("skipped", "đã bỏ qua"),
    ("note conflicts", "xung đột ghi chú"),
    ("Collections", "Bộ sưu tập"),
    ("memberships", "lượt thuộc bộ sưu tập"),
    ("custom chapters", "chương tùy chỉnh"),
    ("flow boundaries", "ranh giới luồng"),
    (
        "Book progress, marks and collections",
        "Tiến độ sách, đánh dấu và bộ sưu tập",
    ),
    ("Reading controls", "Điều khiển đọc"),
    (
        "Custom status bar text",
        "Văn bản thanh trạng thái tùy chỉnh",
    ),
    ("Reading profiles", "Hồ sơ đọc"),
    ("Reading gestures", "Cử chỉ đọc"),
    ("Dictionary preferences", "Tùy chọn từ điển"),
    (
        "Vocabulary and local lookup history",
        "Từ vựng và lịch sử tra cứu trên máy",
    ),
    ("Undated reading totals", "Các tổng số đọc không có ngày"),
    (
        "Daily reading history and calendar",
        "Lịch sử đọc theo ngày và lịch",
    ),
    ("Personal hyphenation rules", "Quy tắc ngắt từ cá nhân"),
    ("Bookmark browser preferences", "Tùy chọn duyệt dấu trang"),
    ("Retained book metadata", "Siêu dữ liệu sách được lưu"),
    ("Read timer preferences", "Tùy chọn hẹn giờ đọc"),
    ("New book defaults", "Mặc định cho sách mới"),
    (
        "User style tweaks and book CSS",
        "Điều chỉnh kiểu người dùng và CSS sách",
    ),
    ("Reading time estimates", "Ước tính thời gian đọc"),
    ("Reading-time estimates", "Ước tính thời gian đọc"),
    ("Reading pace", "Tốc độ đọc"),
];

// Fixed export settings controls; user paths and filename templates stay untouched.
const EXPORT_SETTINGS_LABELS: &[(&str, &str)] = &[
    ("Choose export folder", "Chọn thư mục xuất"),
    ("Reader data / exports", "Dữ liệu Reader / exports"),
    ("Use Reader exports", "Dùng thư mục xuất của Reader"),
    ("Selected", "Đã chọn"),
    ("Default folder", "Thư mục mặc định"),
    ("Export beside the book", "Xuất cạnh tệp sách"),
    (
        "When one book with a source file is exported",
        "Khi xuất một sách có tệp nguồn",
    ),
    ("Use book titles for filenames", "Dùng tên sách làm tên tệp"),
    ("Book title · Field Notes.txt", "Tên sách · Field Notes.txt"),
    (
        "Book ID · 0123456789abcdef.txt",
        "ID sách · 0123456789abcdef.txt",
    ),
    ("Single-book filename", "Tên tệp cho một sách"),
    ("Multiple-book filename", "Tên tệp cho nhiều sách"),
    ("Default name", "Tên mặc định"),
    ("Filename tokens", "Ký hiệu trong tên tệp"),
    ("Use this folder", "Dùng thư mục này"),
    ("Up one folder", "Lên một thư mục"),
    ("Folder", "Thư mục"),
    (
        "Some folders are unavailable",
        "Một số thư mục không khả dụng",
    ),
    ("Title, author, series", "Tên sách, tác giả, bộ sách"),
    (
        "Source filename, full source path",
        "Tên tệp nguồn, đường dẫn đầy đủ của tệp nguồn",
    ),
    (
        "Date (year-month-day), month-day",
        "Ngày (năm-tháng-ngày), tháng-ngày",
    ),
    (
        "Local time (hour:minute), hour-minute-second",
        "Giờ địa phương (giờ:phút), giờ-phút-giây",
    ),
    (
        "Total or last page label, current page label",
        "Nhãn tổng số trang hoặc trang cuối, nhãn trang hiện tại",
    ),
    (
        "Percent read in the book, in the chapter",
        "Phần trăm đã đọc trong sách, trong chương",
    ),
    (
        "Chapter title, pages left in the chapter",
        "Tên chương, số trang còn lại trong chương",
    ),
    (
        "Estimated reading time left in the book, in the chapter",
        "Thời gian đọc ước tính còn lại trong sách, trong chương",
    ),
    (
        "Battery level, battery symbol, footer separator",
        "Mức pin, biểu tượng pin, dấu phân cách chân trang",
    ),
    (
        "Stays literal on its own. %%T writes % followed by the title.",
        "Giữ nguyên khi đứng riêng. %%T ghi % rồi đến tên sách.",
    ),
    (
        "Unknown or unavailable tokens become N-A in filenames.",
        "Ký hiệu không rõ hoặc không khả dụng thành N-A trong tên tệp.",
    ),
    ("Example: %T - %D", "Ví dụ: %T - %D"),
    ("Example: Reading notes %D", "Ví dụ: Reading notes %D"),
    (
        "Save an empty field to use the default name.",
        "Lưu trường trống để dùng tên mặc định.",
    ),
    ("Clear", "Xóa"),
    ("Save", "Lưu"),
    (
        "Enter up to 120 characters on one line",
        "Nhập tối đa 120 ký tự trên một dòng",
    ),
];

// Fixed statistics-reset controls; book names and destructive scopes stay untouched.
const HISTORY_RESET_LABELS: &[(&str, &str)] = &[
    ("Statistics recording is off", "Ghi thống kê đang tắt"),
    (
        "Reset statistics for the current book",
        "Đặt lại thống kê của sách hiện tại",
    ),
    (
        "Reset statistics per book",
        "Đặt lại thống kê theo từng sách",
    ),
    (
        "No other books have statistics",
        "Không có sách nào khác có thống kê",
    ),
    ("No eligible books", "Không có sách đủ điều kiện"),
    (
        "Remove recorded statistics for one book",
        "Xóa thống kê đã ghi của một sách",
    ),
    ("Reset statistics", "Đặt lại thống kê"),
    (
        "Reset statistics for this period",
        "Đặt lại thống kê trong kỳ này",
    ),
    ("Cancel", "Hủy"),
];

// Fixed KOReader statistics-transfer controls; database/book data stay untouched.
const HISTORY_IMPORT_LABELS: &[(&str, &str)] = &[
    (
        "No folders or SQLite databases",
        "Không có thư mục hoặc cơ sở dữ liệu SQLite",
    ),
    (
        "Folder listing reached its limit",
        "Danh sách thư mục đã đạt giới hạn",
    ),
    (
        "Use a closed KOReader database",
        "Dùng cơ sở dữ liệu KOReader đã đóng",
    ),
    (
        "Copy a checkpointed database from KOReader settings to Documents.",
        "Sao chép cơ sở dữ liệu đã được checkpoint từ cài đặt KOReader vào Documents.",
    ),
    ("Preview database", "Xem trước cơ sở dữ liệu"),
    ("Browse Documents", "Duyệt Documents"),
    ("Matched KOReader book", "Sách KOReader khớp"),
    ("Database", "Cơ sở dữ liệu"),
    ("New records", "Bản ghi mới"),
    ("Reading dates", "Ngày đọc"),
    ("Skipped records", "Bản ghi bị bỏ qua"),
    ("Import records", "Nhập bản ghi"),
    (
        "Reading history cannot store these records",
        "Lịch sử đọc không thể lưu các bản ghi này",
    ),
    ("Preview again", "Xem trước lại"),
];

// Calendar month editor fixed labels; typed months and date validation stay untouched.
const HISTORY_MONTH_LABELS: &[(&str, &str)] = &[
    ("Enter month", "Nhập tháng"),
    ("Cancel", "Hủy"),
    ("Clear", "Xóa"),
    (
        "Use YYYY-MM with a month from 01 to 12",
        "Dùng YYYY-MM với tháng từ 01 đến 12",
    ),
    ("Go to month", "Đến tháng"),
];

// Calendar and day-timeline chrome only; recorded data and book titles stay untouched.
const HISTORY_TIMELINE_LABELS: &[(&str, &str)] = &[
    ("Reading calendar", "Lịch đọc"),
    ("Reading timeline", "Dòng thời gian đọc"),
    ("Back", "Quay lại"),
    ("Summary", "Tổng quan"),
    ("Daily history", "Lịch sử theo ngày"),
    ("Today", "Hôm nay"),
    ("Settings", "Cài đặt"),
    ("All books", "Tất cả sách"),
    ("Sun", "CN"),
    ("Mon", "T2"),
    ("Tue", "T3"),
    ("Wed", "T4"),
    ("Thu", "T5"),
    ("Fri", "T6"),
    ("Sat", "T7"),
    (
        "No recorded reading in this day.",
        "Không có lượt đọc được ghi trong ngày này.",
    ),
    ("Earlier sessions", "Phiên trước"),
    ("Later sessions", "Phiên sau"),
];

// Fixed weekly-reading progress controls; calculated values and dates stay untouched.
const HISTORY_PROGRESS_LABELS: &[(&str, &str)] = &[
    ("Reading progress", "Tiến độ đọc"),
    ("Close", "Đóng"),
    (
        "No reading recorded in the last week.",
        "Không có lượt đọc được ghi trong tuần qua.",
    ),
    ("Last week", "Tuần qua"),
    ("Total pages", "Tổng số trang"),
    ("Total time", "Tổng thời gian"),
    ("Pages per day", "Trang mỗi ngày"),
    ("Time per day", "Thời gian mỗi ngày"),
    ("Week progress", "Tiến độ trong tuần"),
    ("Session", "Phiên"),
    ("Today", "Hôm nay"),
    ("Pages", "Trang"),
    ("Time", "Thời gian"),
];

// Fixed statistics report controls; book titles and recorded values stay untouched.
const HISTORY_REPORT_LABELS: &[(&str, &str)] = &[
    ("Book filter", "Lọc theo sách"),
    ("All books", "Tất cả sách"),
    ("Books by week", "Sách theo tuần"),
    ("Books by month", "Sách theo tháng"),
    ("Last week", "Tuần qua"),
    ("Last month by day", "Tháng qua theo ngày"),
    ("Last year by day", "Năm qua theo ngày"),
    ("Last year by week", "Năm qua theo tuần"),
    ("All stats by month", "Mọi thống kê theo tháng"),
    (
        "No dated reading sessions yet",
        "Chưa có phiên đọc ghi ngày",
    ),
    (
        "Hold a book to reset its time in this period",
        "Nhấn giữ sách để đặt lại thời gian của sách trong kỳ này",
    ),
    ("Recorded reading time", "Thời gian đọc đã ghi"),
    ("Distinct reading screens", "Số màn hình đọc khác nhau"),
    ("Continue reading", "Tiếp tục đọc"),
    (
        "No reading sessions in this report",
        "Không có phiên đọc trong báo cáo này",
    ),
    ("Total time spent reading", "Tổng thời gian đọc"),
    (
        "Hold a book to reset its reading time for this day",
        "Nhấn giữ sách để đặt lại thời gian đọc của sách trong ngày này",
    ),
    ("All recorded sessions", "Mọi phiên đã ghi"),
];

// Fixed statistics summary controls only; book titles and recorded values stay untouched.
const HISTORY_SUMMARY_LABELS: &[(&str, &str)] = &[
    ("All books", "Tất cả sách"),
    ("Choose books", "Chọn sách"),
    ("Recorded reading time", "Thời gian đọc đã ghi"),
    ("Distinct reading screens", "Số màn hình đọc khác nhau"),
    (
        "Page time, capped per screen",
        "Thời gian trang, giới hạn theo màn hình",
    ),
    (
        "Earlier screen time · undated",
        "Thời gian màn hình trước đây · không có ngày",
    ),
    (
        "Uncertain clock · undated",
        "Đồng hồ không chắc chắn · không có ngày",
    ),
    ("Daily history", "Lịch sử theo ngày"),
    ("Time range", "Khoảng thời gian"),
    ("Reading progress", "Tiến độ đọc"),
    ("Calendar", "Lịch"),
    ("Today's timeline", "Dòng thời gian hôm nay"),
    ("Statistics settings", "Cài đặt thống kê"),
    ("Import KOReader statistics", "Nhập thống kê KOReader"),
    ("Continue reading", "Tiếp tục đọc"),
    (
        "Resume this book's saved position",
        "Tiếp tục từ vị trí đã lưu của sách này",
    ),
];

// Fixed reading-statistics settings; recorded data and destructive reset prompts stay untouched.
const HISTORY_SETTINGS_LABELS: &[(&str, &str)] = &[
    ("Reading statistics", "Thống kê đọc"),
    ("On", "Bật"),
    ("Off", "Tắt"),
    ("Read page duration limits", "Giới hạn thời gian đọc trang"),
    (
        "Freeze finished books",
        "Ngừng ghi thống kê sách đã đọc xong",
    ),
    ("Hourly histogram", "Biểu đồ theo giờ"),
    ("Browse future months", "Duyệt các tháng tương lai"),
    ("Reset statistics", "Đặt lại thống kê"),
    (
        "Visit filters apply to new reading",
        "Bộ lọc lượt đọc áp dụng cho lần đọc mới",
    ),
    (
        "Quick skims are ignored. Rereads retain their own sessions.",
        "Bỏ qua các lượt đọc lướt nhanh. Lần đọc lại giữ các phiên riêng.",
    ),
    ("Minimum", "Tối thiểu"),
    ("Maximum", "Tối đa"),
    (
        "Cap visits above the maximum. Otherwise, skip visits below the minimum.",
        "Giới hạn lượt đọc vượt mức tối đa. Nếu không, bỏ qua lượt đọc dưới mức tối thiểu.",
    ),
    ("Defaults", "Mặc định"),
    ("Cancel", "Hủy"),
    ("Apply", "Áp dụng"),
    ("Use value", "Dùng giá trị"),
    ("Delete", "Xóa"),
];

// Fixed reading-statistics calendar settings; dates and recorded data are untouched.
const HISTORY_CALENDAR_LABELS: &[(&str, &str)] = &[
    ("Calendar week starts", "Tuần trong lịch bắt đầu vào"),
    ("Books per calendar day", "Số sách mỗi ngày trên lịch"),
    ("Sunday", "Chủ nhật"),
    ("Monday", "Thứ hai"),
    ("Tuesday", "Thứ ba"),
    ("Wednesday", "Thứ tư"),
    ("Thursday", "Thứ năm"),
    ("Friday", "Thứ sáu"),
    ("Saturday", "Thứ bảy"),
    (
        "Daily timeline starts at",
        "Dòng thời gian mỗi ngày bắt đầu lúc",
    ),
    (
        "Also use in calendar view",
        "Cũng áp dụng cho chế độ xem lịch",
    ),
    ("On", "Bật"),
    ("Off", "Tắt"),
    ("Selected", "Đã chọn"),
    (
        "Start after midnight to keep late-night reading with the previous evening.",
        "Bắt đầu sau nửa đêm để tính việc đọc khuya vào buổi tối hôm trước.",
    ),
    ("Hour", "Giờ"),
    ("Minute", "Phút"),
    ("Cancel", "Hủy"),
    ("Set time", "Đặt giờ"),
];

// Annotation-transfer settings only; folder paths and import previews stay untouched.
const ANNOTATION_EXCHANGE_LABELS: &[(&str, &str)] = &[
    (
        "Export annotations on book closing",
        "Xuất chú thích khi đóng sách",
    ),
    (
        "Save a local KOReader transfer when the book closes",
        "Lưu bản chuyển KOReader trên máy khi đóng sách",
    ),
    (
        "Keep all annotations on import",
        "Giữ mọi chú thích khi nhập",
    ),
    (
        "Keep older local marks missing from the transfer",
        "Giữ các đánh dấu cũ trên máy không có trong bản chuyển",
    ),
    ("Export / import folder", "Thư mục xuất / nhập"),
    ("Book metadata folder", "Thư mục siêu dữ liệu sách"),
    ("Use book metadata folder", "Dùng thư mục siêu dữ liệu sách"),
    (
        "The book’s adjacent .sdr folder",
        "Thư mục .sdr cạnh tệp sách",
    ),
    ("Import annotation transfer", "Nhập bản chuyển chú thích"),
    (
        "Review changes to bookmarks, highlights and notes",
        "Xem lại thay đổi đối với dấu trang, phần tô sáng và ghi chú",
    ),
    ("Export annotation transfer", "Xuất bản chuyển chú thích"),
    (
        "Save a KOReader .annotations.lua file for this book",
        "Lưu tệp KOReader .annotations.lua cho sách này",
    ),
];

// Fixed typography settings labels; publisher mode names remain unchanged.
const TYPOGRAPHY_SETTINGS_LABELS: &[(&str, &str)] = &[
    ("Use default", "Dùng mặc định"),
    ("Selected", "Đã chọn"),
    ("Chapter first", "Chương trước"),
    ("Whole book first", "Toàn bộ sách trước"),
    (
        "Read the edited chapter while the page index builds",
        "Đọc chương đã chỉnh trong khi tạo chỉ mục trang",
    ),
    (
        "Keep the old page until all supported chapters are ready",
        "Giữ trang cũ đến khi mọi chương được hỗ trợ đã sẵn sàng",
    ),
    ("Default for EPUBs", "Mặc định cho EPUB"),
    (
        "Chapter first for this layout",
        "Dùng chương trước cho bố cục này",
    ),
    (
        "Whole book first needs a bounded horizontal text EPUB",
        "Toàn bộ sách trước cần EPUB văn bản ngang có kích thước giới hạn",
    ),
    ("Cancel preparation", "Hủy chuẩn bị"),
    ("For new horizontal EPUBs", "Cho EPUB ngang mới"),
    (
        "Complex layouts may use Reader’s established layout",
        "Bố cục phức tạp có thể dùng bố cục hiện có của Reader",
    ),
    ("Reader’s established layout", "Bố cục hiện có của Reader"),
    (
        "Separate margins and empty breaks",
        "Lề tách biệt và ngắt trống",
    ),
    ("Joined margins and inline boxes", "Lề gộp và hộp nội dòng"),
    (
        "Float wrapping and text-sized boxes",
        "Bao quanh phần tử nổi và hộp theo cỡ chữ",
    ),
    (
        "Publisher sizes and signed margins",
        "Kích thước của nhà xuất bản và lề có dấu",
    ),
];

// Fixed scrolling submenu labels; the chosen method and delay remain unchanged.
const SCROLLING_SETTINGS_LABELS: &[(&str, &str)] = &[
    ("Classic scrolling", "Cuộn cổ điển"),
    ("Turbo scrolling", "Cuộn tăng tốc"),
    ("On-release scrolling", "Cuộn khi nhấc tay"),
    (
        "Move the page with your finger",
        "Di chuyển trang theo ngón tay",
    ),
    (
        "Move farther as your finger moves away from its starting point",
        "Di chuyển xa hơn khi ngón tay ra xa điểm bắt đầu",
    ),
    (
        "Move the page when you lift your finger",
        "Di chuyển trang khi bạn nhấc ngón tay",
    ),
    ("Selected", "Đã chọn"),
    ("Activation delay", "Độ trễ kích hoạt"),
];

// Fixed navigation settings and Back policies; stored choices are unchanged.
const NAVIGATION_SETTINGS_LABELS: &[(&str, &str)] = &[
    (
        "Add opening page to location history",
        "Thêm trang mở vào lịch sử vị trí",
    ),
    (
        "Uses your saved place when reopening a book",
        "Dùng vị trí đã lưu khi mở lại sách",
    ),
    ("Skim dialog position", "Vị trí cửa sổ đọc lướt"),
    ("Top", "Trên"),
    ("Center", "Giữa"),
    ("Bottom", "Dưới"),
    ("Selected", "Đã chọn"),
    ("Assign a Back gesture", "Gán cử chỉ Quay lại"),
    (
        "Choose a gesture, then Back (navigation settings)",
        "Chọn một cử chỉ, rồi chọn Quay lại (cài đặt điều hướng)",
    ),
    ("Back to exit", "Quay lại để thoát"),
    ("Back in file browser", "Quay lại trong trình duyệt tệp"),
    ("Back in Reader", "Quay lại trong Reader"),
    ("Ask before exiting", "Hỏi trước khi thoát"),
    ("Exit immediately", "Thoát ngay"),
    ("Don't exit", "Không thoát"),
    ("Follow Back to exit", "Theo cài đặt Quay lại để thoát"),
    ("Go to parent folder", "Đến thư mục cha"),
    ("Go to previous location", "Đến vị trí trước"),
    ("Go to previous read page", "Đến trang đã đọc trước"),
    ("Go to file browser", "Đến trình duyệt tệp"),
];

// Fixed page-turn settings labels; stored choices and percentages stay unchanged.
const PAGE_TURN_SETTINGS_LABELS: &[(&str, &str)] = &[
    ("Default", "Mặc định"),
    ("Left / right", "Trái / phải"),
    ("Top / bottom", "Trên / dưới"),
    ("Bottom / top", "Dưới / trên"),
    ("Follow the publication", "Theo sách"),
    ("Left to right", "Trái sang phải"),
    ("Right to left", "Phải sang trái"),
    (
        "Backward and forward zone sizes",
        "Kích thước vùng lùi và tiến",
    ),
    (
        "Invert document-related dialogs",
        "Đảo hướng hộp thoại tài liệu",
    ),
    ("Mirror taps only", "Chỉ đảo hướng chạm"),
    (
        "Retained from an earlier version",
        "Được giữ lại từ phiên bản trước",
    ),
    ("With taps", "Bằng cách chạm"),
    ("With swipes", "Bằng cách vuốt"),
    ("Swipes always active", "Luôn bật vuốt"),
    (
        "Enable page swipes to change this setting",
        "Bật vuốt lật trang để thay đổi cài đặt này",
    ),
    (
        "Turn pages with horizontal swipes while touch is locked",
        "Vuốt ngang để lật trang khi cảm ứng bị khóa",
    ),
    ("Tap zones", "Vùng chạm"),
    (
        "Enable page taps to choose a layout",
        "Bật chạm lật trang để chọn bố cục",
    ),
    (
        "Invert page turn taps and swipes",
        "Đảo hướng chạm và vuốt lật trang",
    ),
    (
        "Contents, Book Map and Page Browser",
        "Mục lục, Sơ đồ sách và Duyệt trang",
    ),
    ("Defaults for new books", "Mặc định cho sách mới"),
];

// Fixed link-settings controls only; link addresses and book text stay untouched.
const LINK_SETTINGS_LABELS: &[(&str, &str)] = &[
    ("Tap to follow links", "Chạm để mở liên kết"),
    (
        "Allow larger tap area around links",
        "Mở rộng vùng chạm quanh liên kết",
    ),
    (
        "Ignore external links on tap",
        "Bỏ qua liên kết ngoài khi chạm",
    ),
    ("Swipe to go back", "Vuốt để quay lại"),
    (
        "Swipe to follow nearest link",
        "Vuốt để mở liên kết gần nhất",
    ),
    (
        "Ignore external links on swipe",
        "Bỏ qua liên kết ngoài khi vuốt",
    ),
    (
        "Swipe to jump to latest bookmark",
        "Vuốt đến dấu trang mới nhất",
    ),
    (
        "Use book font in popups",
        "Dùng phông chữ của sách trong cửa sổ nổi",
    ),
    ("Justify text in popups", "Căn đều chữ trong cửa sổ nổi"),
    (
        "Footnote popup font size",
        "Cỡ chữ chú thích trong cửa sổ nổi",
    ),
    (
        "Available when tap to follow links is on",
        "Khả dụng khi bật chạm để mở liên kết",
    ),
    (
        "Available when swipe to follow nearest link is on",
        "Khả dụng khi bật vuốt để mở liên kết gần nhất",
    ),
    (
        "Available when footnote previews are on",
        "Khả dụng khi bật xem trước chú thích",
    ),
    (
        "Available when tap or swipe to follow links is on",
        "Khả dụng khi bật chạm hoặc vuốt để mở liên kết",
    ),
    (
        "Swipe right to return to your previous location",
        "Vuốt sang phải để về vị trí trước",
    ),
    (
        "Swipe left from near a link to follow it",
        "Vuốt sang trái gần liên kết để mở",
    ),
    (
        "Swipe left when there is no link to follow",
        "Vuốt sang trái khi không có liên kết để mở",
    ),
];

// Notification-source controls only; notification messages are user-visible data.
const NOTIFICATION_SETTINGS_LABELS: &[(&str, &str)] = &[
    ("Bottom menu icons", "Biểu tượng trình đơn dưới"),
    ("Bottom menu toggles", "Công tắc trình đơn dưới"),
    ("Bottom menu ± buttons", "Nút ± trình đơn dưới"),
    (
        "Bottom menu custom values",
        "Giá trị tùy chỉnh trình đơn dưới",
    ),
    ("Bottom menu progress bars", "Thanh tiến độ trình đơn dưới"),
    ("Gestures and profiles", "Cử chỉ và hồ sơ"),
    ("Other sources", "Nguồn khác"),
    ("Show past notifications", "Hiện thông báo trước"),
    ("No notifications yet", "Chưa có thông báo"),
];

fn valid_selection_seconds(value: &str) -> bool {
    let number = value.strip_prefix(['+', '-', '−']).unwrap_or(value);
    let (whole, fraction) = number
        .split_once('.')
        .map_or((number, None), |(whole, fraction)| (whole, Some(fraction)));
    !whole.is_empty()
        && whole.bytes().all(|digit| digit.is_ascii_digit())
        && fraction.is_none_or(|digits| {
            !digits.is_empty() && digits.bytes().all(|digit| digit.is_ascii_digit())
        })
}

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
    pub(super) fn reading_font_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) =
                READING_FONT_LABELS.iter().find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn footer_settings_status<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = FOOTER_SETTINGS_STATUSES
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn footer_settings_title<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = FOOTER_SETTINGS_TITLES
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn footer_settings_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = FOOTER_SETTINGS_TITLES
                .iter()
                .find(|(_, value)| *value == label)
            {
                return english;
            }
        }
        label
    }

    pub(super) fn status_bar_settings_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = STATUS_BAR_SETTINGS_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn status_bar_settings_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = STATUS_BAR_SETTINGS_LABELS
                .iter()
                .find(|(_, translated)| *translated == label)
            {
                return english;
            }
        }
        label
    }

    pub(super) fn auto_turn_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = AUTO_TURN_LABELS.iter().find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn auto_turn_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = AUTO_TURN_LABELS
                .iter()
                .find(|(_, translated)| *translated == label)
            {
                return english;
            }
        }
        label
    }

    pub(super) fn selection_settings_text(self, value: &str) -> String {
        if self == Self::Vietnamese {
            if let Some(number) = value.strip_suffix(" seconds") {
                if valid_selection_seconds(number) {
                    return format!("{number} giây");
                }
            }
            if let Some((_, vietnamese)) = SELECTION_SETTINGS_LABELS
                .iter()
                .find(|(key, _)| *key == value)
            {
                return (*vietnamese).to_owned();
            }
        }
        value.to_owned()
    }

    pub(super) fn selection_settings_english(self, value: &str) -> String {
        if self == Self::Vietnamese {
            if let Some(number) = value.strip_suffix(" giây") {
                if valid_selection_seconds(number) {
                    return format!("{number} seconds");
                }
            }
            if let Some((english, _)) = SELECTION_SETTINGS_LABELS
                .iter()
                .find(|(_, translated)| *translated == value)
            {
                return (*english).to_owned();
            }
        }
        value.to_owned()
    }

    pub(super) fn overview_page_action_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = OVERVIEW_PAGE_ACTION_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn bookmark_list_settings_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = BOOKMARK_LIST_SETTINGS_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn bookmark_list_settings_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = BOOKMARK_LIST_SETTINGS_LABELS
                .iter()
                .find(|(_, value)| *value == label)
            {
                return english;
            }
        }
        label
    }

    pub(super) fn bookmark_list_settings_detail(self, detail: &str) -> String {
        if self == Self::Vietnamese {
            if let Some(number) = detail
                .strip_prefix("Automatic (")
                .and_then(|s| s.strip_suffix(')'))
            {
                if !number.is_empty() && number.bytes().all(|digit| digit.is_ascii_digit()) {
                    return format!("Tự động ({number})");
                }
            }
        }
        self.bookmark_list_settings_text(detail).to_owned()
    }

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
            "Add current location" => "Thêm vị trí hiện tại",
            "Add from bookmark" => "Thêm từ dấu trang",
            "Edit chapters" => "Sửa các chương",
            "Use publisher contents" => "Dùng mục lục nhà xuất bản",
            "Use custom contents" => "Dùng mục lục tùy chỉnh",
            "Page browser" => "Duyệt trang",
            "Hold a page to add or edit a chapter" => "Nhấn giữ trang để thêm hoặc sửa chương",
            "Edit chapter" => "Sửa chương",
            "Add chapter" => "Thêm chương",
            "Chapter title" => "Tên chương",
            "Save" => "Lưu",
            "Cancel" => "Hủy",
            "Remove chapter" => "Xóa chương",
            "Starts on page" => "Bắt đầu ở trang",
            "Remove this chapter?" => "Xóa chương này?",
            "You can add this location again from Contents." => {
                "Bạn có thể thêm lại vị trí này từ Mục lục."
            }
            "Remove" => "Xóa",
            "Use document headings?" => "Dùng tiêu đề tài liệu?",
            "Restore publisher contents?" => "Khôi phục mục lục nhà xuất bản?",
            "Build the table of contents from this book's headings." => {
                "Tạo mục lục từ các tiêu đề trong sách này."
            }
            "Switch back to the table of contents supplied with this book." => {
                "Chuyển lại mục lục được cung cấp cùng sách này."
            }
            "Use headings" => "Dùng tiêu đề",
            "Restore" => "Khôi phục",
            "Building the outline" => "Đang tạo dàn mục",
            "No readable chapters found" => "Không tìm thấy chương có thể đọc",
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
            "Remove notes" => "Xóa ghi chú",
            "Remove marks" => "Xóa dấu",
            "Choose marks" => "Chọn dấu",
            "No marks selected" => "Chưa chọn dấu nào",
            "Remove matching marks (this book)" => "Xóa dấu phù hợp (sách này)",
            "Selected marks and their notes will be deleted" => {
                "Dấu đã chọn và ghi chú của chúng sẽ bị xóa"
            }
            "Delete marks" => "Xóa dấu",
            "Marks removed" => "Đã xóa dấu",
            "Edit note" => "Sửa ghi chú",
            "Edit" => "Sửa",
            "Remove" => "Xóa",
            "Remove mark" => "Xóa dấu",
            "Remove this mark?" => "Xóa dấu này?",
            "This mark and its note will be deleted." => "Dấu này và ghi chú của nó sẽ bị xóa.",
            "Remove note" => "Xóa ghi chú",
            "Save note" => "Lưu ghi chú",
            "Insert quote" => "Chèn trích dẫn",
            "Copy quote" => "Chép đoạn",
            "Copy text" => "Chép chữ",
            "Copy note" => "Chép ghi chú",
            "Text copied" => "Đã chép chữ",
            "Note copied" => "Đã chép ghi chú",
            "Quote" => "Trích dẫn",
            "Paste" => "Dán",
            "Save" => "Lưu",
            "New line" => "Dòng mới",
            "Keep editing" => "Tiếp tục sửa",
            "Note saved" => "Đã lưu ghi chú",
            "Remove this note? The saved mark will remain." => {
                "Xóa ghi chú này? Dấu đã lưu vẫn được giữ."
            }
            "Page" => "Trang",
            "Chapter" => "Chương",
            "Choose notes" => "Chọn ghi chú",
            "Clear selection" => "Bỏ chọn tất cả",
            "Not selected" => "Chưa chọn",
            "No notes selected" => "Chưa chọn ghi chú nào",
            "Select all" => "Chọn tất cả",
            "Select page" => "Chọn trang này",
            "Done" => "Xong",
            "Remove matching notes (this book)" => "Xóa ghi chú phù hợp (sách này)",
            "Bookmarks and highlights will remain" => "Dấu trang và đoạn tô sáng vẫn được giữ",
            "Delete notes" => "Xóa ghi chú",
            "Notes removed" => "Đã xóa ghi chú",
            "Cancel" => "Hủy",
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
            "Current book" => "Sách đang đọc",
            "Show current page in results" => "Hiện trang đang đọc trong kết quả",
            "Latest saved mark" => "Dấu đã lưu mới nhất",
            "No matching marks in the current book" => "Không có dấu phù hợp trong sách đang đọc",
            "No matching marks" => "Không có dấu phù hợp",
            "No matching marks in the current chapter" => {
                "Không có dấu phù hợp trong chương đang đọc"
            }
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
            "Case" => "Hoa/thường",
            "Search options" => "Tùy chọn tìm kiếm",
            "Types filtered" => "Đang lọc loại dấu",
            "Bookmark" => "Dấu trang",
            "Bookmark note" => "Ghi chú dấu trang",
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
            "Close search" => "Đóng tìm kiếm",
            "Searching pages" => "Đang tìm trong các trang",
            "Searching earlier text" => "Đang tìm phần văn bản trước",
            "Searching later text" => "Đang tìm phần văn bản sau",
            "No match found. Some pages could not be searched" => {
                "Không tìm thấy kết quả. Một số trang không thể tìm kiếm"
            }
            "No earlier matching page" => "Không có trang khớp trước đó",
            "No later matching page" => "Không có trang khớp tiếp theo",
            "Showing the first 5,000 matches on this page" => {
                "Đang hiện 5.000 kết quả đầu tiên trên trang này"
            }
            "Search result memory limit reached" => "Đã đạt giới hạn bộ nhớ kết quả tìm kiếm",
            "Showing first 5,000 matches" => "Đang hiện 5.000 kết quả đầu tiên",
            "Some chapters could not be searched" => "Một số chương không thể tìm kiếm",
            "No earlier match" => "Không có kết quả trước đó",
            "No later match" => "Không có kết quả tiếp theo",
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
            "Marks" => "Dấu",
            "Mark browsing" => "Duyệt dấu",
            "Swipe or use page keys to browse marks" => "Vuốt hoặc dùng nút trang để duyệt dấu",
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
            "No earlier saved mark" => "Không có dấu đã lưu trước đó",
            "No later saved mark" => "Không có dấu đã lưu tiếp theo",
            "No earlier page bookmark" => "Không có dấu trang trước đó",
            "No later page bookmark" => "Không có dấu trang tiếp theo",
            "Finding page bookmark" => "Đang tìm dấu trang",
            "Finding saved mark" => "Đang tìm dấu đã lưu",
            "Saved marks changed; try again" => "Dấu đã lưu đã thay đổi; hãy thử lại",
            "Folder book changed or cannot be safely checked" => {
                "Sách thư mục đã đổi hoặc không thể kiểm tra an toàn"
            }
            "The book or bookmarks changed; try again" => "Sách hoặc dấu trang đã đổi; hãy thử lại",
            "The book changed; try again" => "Sách đã thay đổi; hãy thử lại",
            "The book or reading settings changed; try again" => {
                "Sách hoặc cài đặt đọc đã đổi; hãy thử lại"
            }
            "Bookmarks exceed the search memory limit" => "Dấu trang vượt giới hạn bộ nhớ tìm kiếm",
            "Unable to start bookmark search" => "Không thể bắt đầu tìm dấu trang",
            "Previous bookmark search is still stopping" => "Lượt tìm dấu trang trước đang dừng",
            "Bookmark search stopped" => "Tìm dấu trang đã dừng",
            "A bookmarked chapter could not be checked" => "Không thể kiểm tra chương có dấu trang",
            "This book cannot be verified for bookmark navigation" => {
                "Không thể xác minh sách để chuyển dấu trang"
            }
            "The bookmarked page changed; try again" => "Trang đã đánh dấu thay đổi; hãy thử lại",
            "Saved mark location is unavailable" => "Vị trí dấu đã lưu không khả dụng",
            _ => english,
        }
    }

    /// Fixed Page Browser labels, never book chapters or publisher labels.
    pub(super) fn overview_text<'a>(self, english: &'a str) -> &'a str {
        if self != Self::Vietnamese {
            return english;
        }
        match english {
            "Pages" => "Trang",
            "Back" => "Quay lại",
            "Grid" => "Lưới",
            "Page browser grid" => "Lưới duyệt trang",
            "Return to book" => "Về trang đọc",
            "Page browser help" => "Trợ giúp duyệt trang",
            "Help" => "Trợ giúp",
            "Tap a thumbnail to read. Hold it to bookmark or show page actions." => {
                "Chạm ảnh thu nhỏ để đọc. Giữ để đánh dấu hoặc xem tùy chọn trang."
            }
            "Swipe vertically for one row, horizontally or use page buttons for a screen." => {
                "Vuốt dọc để chuyển một hàng; vuốt ngang hoặc nhấn nút trang để chuyển một màn hình."
            }
            "Swipe along the top or left edge to change the grid columns or rows." => {
                "Vuốt dọc mép trên hoặc mép trái để đổi số cột hoặc hàng."
            }
            "Tap or drag the ribbon to browse. Hold it to open the book map." => {
                "Chạm hoặc kéo thanh để duyệt. Giữ thanh để mở sơ đồ sách."
            }
            "The handle is browsing; the caret is reading. Squares show page or chapter history." => {
                "Nút kéo là vị trí đang duyệt; mũi tên là vị trí đọc. Ô vuông là lịch sử trang hoặc chương."
            }
            "Ribbon: gray highlight, black note, tall bookmark. Reflow marks show starting chapters." => {
                "Thanh duyệt: xám là tô sáng, đen là ghi chú, vạch cao là dấu trang. Sách dàn trang lại hiện chương bắt đầu."
            }
            "Numbers and chapter levels save per book. Defaults apply to page numbers." => {
                "Số trang và cấp chương được lưu theo sách. Mặc định áp dụng cho số trang."
            }
            "Done" => "Xong",
            "Thumbnail columns" => "Số cột ảnh thu nhỏ",
            "Thumbnail rows" => "Số hàng ảnh thu nhỏ",
            "Thumbnail page numbers" => "Số trang ảnh thu nhỏ",
            "Chapter levels" => "Cấp chương",
            "Save number default" => "Lưu mặc định số trang",
            "Use default numbers" => "Dùng số trang mặc định",
            "None" => "Không",
            "Row start" => "Đầu hàng",
            "All" => "Tất cả",
            "Set as default" => "Đặt mặc định",
            "Use default" => "Dùng mặc định",
            "Default saved" => "Đã lưu mặc định",
            "Reading" => "Đang đọc",
            "Page browser on tap" => "Chạm để duyệt trang",
            "Page" => "Trang",
            "Hidden" => "Đã ẩn",
            "Preview unavailable" => "Không có bản xem trước",
            "Previous" => "Trước",
            "Prev chapter" => "Chương trước",
            "Next" => "Tiếp",
            "Next chapter" => "Chương tiếp",
            "Skim pages" => "Lướt trang",
            "Skim chapters" => "Lướt chương",
            "of" => "trên",
            _ => english,
        }
    }

    pub(super) fn overview_page_count(self, count: usize) -> String {
        if self == Self::Vietnamese {
            format!("{count} trang")
        } else {
            format!("{count} pages")
        }
    }

    /// Fixed Book Map labels; chapter and publisher labels remain untouched.
    pub(super) fn book_map_text<'a>(self, english: &'a str) -> &'a str {
        if self != Self::Vietnamese {
            return english;
        }
        match english {
            "Book map" => "Sơ đồ sách",
            "Includes this session" => "Có phiên này",
            "Reading time (partial)" => "Thời gian đọc (một phần)",
            "Book map help" => "Trợ giúp sơ đồ sách",
            "Return to book" => "Về trang đọc",
            "Help" => "Trợ giúp",
            "Done" => "Xong",
            "Tap a group to zoom. Tap one page to preview or read it." => {
                "Chạm nhóm để phóng to. Chạm một trang để xem trước hoặc đọc."
            }
            "Page buttons move groups. Back returns to the prior view." => {
                "Dùng nút trang để chuyển nhóm. Quay lại mở màn hình trước."
            }
            "Bookmark, marker and pencil icons indicate bookmarks, highlights and notes." => {
                "Biểu tượng dấu trang, bút tô và bút chì biểu thị dấu trang, tô sáng và ghi chú."
            }
            "Reading-time bars show visits; gray includes this session when known." => {
                "Thanh thời gian cho biết lượt đọc; màu xám có lượt trong phiên này khi xác định được."
            }
            "Filled numbered badges are previous locations; outlines are next locations." => {
                "Ô số đặc là vị trí trước; ô có viền là vị trí sau."
            }
            "Triangle: reading page. Small outline: browser focus. Shading: hidden pages." => {
                "Tam giác: trang đang đọc. Ô viền nhỏ: trang đang duyệt. Nền xám: trang ẩn."
            }
            "In a crowded slot, a note icon can replace a highlight icon." => {
                "Khi ô chật, biểu tượng ghi chú có thể thay biểu tượng tô sáng."
            }
            "Return to book closes nested maps without moving the reading page." => {
                "Về trang đọc đóng các sơ đồ lồng nhau mà không đổi trang đang đọc."
            }
            "Back" => "Quay lại",
            "Pages" => "Trang",
            "Reading page" => "Trang đang đọc",
            "Reading time" => "Thời gian đọc",
            "Gray: this session" => "Xám: phiên này",
            "Filled: previous; outline: next" => "Đặc: vị trí trước; viền: vị trí sau",
            "Map focus" => "Vị trí đang chọn",
            "Hidden" => "Đã ẩn",
            "Includes hidden pages" => "Có trang bị ẩn",
            "Current position" => "Vị trí hiện tại",
            "Checking saved marks" => "Đang kiểm tra dấu đã lưu",
            "Bookmarks" => "Dấu trang",
            "Highlights" => "Đoạn tô sáng",
            "Notes" => "Ghi chú",
            "No entries" => "Không có mục",
            "Previous" => "Trước",
            "Next" => "Tiếp",
            _ => english,
        }
    }

    /// Fixed annotation detail and note-editor labels only.
    pub(super) fn annotation_text<'a>(self, english: &'a str) -> &'a str {
        if self != Self::Vietnamese {
            return english;
        }
        match english {
            "Highlight" => "Đoạn tô sáng",
            "Remove highlight?" => "Xóa tô sáng?",
            "Add note" => "Thêm ghi chú",
            "Edit note" => "Sửa ghi chú",
            "Back" => "Quay lại",
            "Read passage" => "Đọc đoạn văn",
            "Extend highlight" => "Mở rộng vùng tô sáng",
            "Remove highlight" => "Xóa tô sáng",
            "Chapter" => "Chương",
            "Remove highlight and note" => "Xóa tô sáng và ghi chú",
            "Keep highlight" => "Giữ vùng tô sáng",
            "Note text" => "Nội dung ghi chú",
            "Save" => "Lưu",
            _ => english,
        }
    }

    pub(super) fn chapter_navigation_fixed<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = CHAPTER_NAVIGATION_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    /// This screen generates levels and heading counts from numeric indices.
    pub(super) fn chapter_navigation_text(self, english: &str) -> String {
        if self == Self::Vietnamese {
            if let Some(number) = english.strip_prefix("Contents level ")
                && !number.is_empty()
                && number.bytes().all(|byte| byte.is_ascii_digit())
            {
                return format!("Cấp mục lục {number}");
            }
            if let Some(number) = english
                .strip_suffix(" headings")
                .or_else(|| english.strip_suffix(" heading"))
                && !number.is_empty()
                && number.bytes().all(|byte| byte.is_ascii_digit())
            {
                return format!("{number} tiêu đề");
            }
        }
        self.chapter_navigation_fixed(english).to_owned()
    }

    pub(super) fn chapter_navigation_english(self, label: &str) -> String {
        if self == Self::Vietnamese {
            if let Some(number) = label.strip_prefix("Cấp mục lục ")
                && !number.is_empty()
                && number.bytes().all(|byte| byte.is_ascii_digit())
            {
                return format!("Contents level {number}");
            }
            if let Some((english, _)) = CHAPTER_NAVIGATION_LABELS
                .iter()
                .find(|(_, value)| *value == label)
            {
                return (*english).to_owned();
            }
        }
        label.to_owned()
    }

    pub(super) fn contents_settings_text(self, english: &str) -> String {
        if self == Self::Vietnamese {
            if let Some(number) = english
                .strip_prefix("Automatic (")
                .and_then(|s| s.strip_suffix(')'))
                && !number.is_empty()
                && number.bytes().all(|byte| byte.is_ascii_digit())
            {
                return format!("Tự động ({number})");
            }
            if let Some((_, vietnamese)) = CONTENTS_SETTINGS_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return (*vietnamese).to_owned();
            }
        }
        english.to_owned()
    }

    pub(super) fn contents_settings_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = CONTENTS_SETTINGS_LABELS
                .iter()
                .find(|(_, value)| *value == label)
            {
                return english;
            }
        }
        label
    }

    pub(super) fn numbering_fixed<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = NUMBERING_LABELS.iter().find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    /// Localize generated progress descriptions, retaining every numeric value.
    pub(super) fn numbering_text(self, english: &str) -> String {
        if self == Self::Vietnamese {
            let digits = |s: &str| !s.is_empty() && s.bytes().all(|byte| byte.is_ascii_digit());
            if let Some(number) = english.strip_suffix(" labels supplied by this edition")
                && digits(number)
            {
                return format!("{number} nhãn từ ấn bản này");
            }
            if let Some(number) = english.strip_suffix(" characters per stable page")
                && digits(number)
            {
                return format!("{number} ký tự mỗi trang cố định");
            }
            if let Some((progress, stride)) = english.split_once(" · ")
                && let Some((done, total)) = progress.split_once(" of ")
                && let Some(stride) = stride.strip_suffix(" characters per page")
                && digits(done)
                && digits(total)
                && digits(stride)
            {
                return format!("{done} trên {total} · {stride} ký tự mỗi trang");
            }
        }
        self.numbering_fixed(english).to_owned()
    }

    pub(super) fn numbering_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = NUMBERING_LABELS.iter().find(|(_, value)| *value == label) {
                return english;
            }
        }
        label
    }

    /// Fixed labels surrounding publisher-supplied page data only.
    pub(super) fn publisher_page_text<'a>(self, english: &'a str) -> &'a str {
        if self != Self::Vietnamese {
            return english;
        }
        match english {
            "Print edition source" => "Nguồn ấn bản in",
            "Show numbers in footer and margin" => "Hiện số ở chân trang và lề",
            "Continue reading" => "Đọc tiếp",
            _ => english,
        }
    }

    pub(super) fn publisher_page_offer_count(self, count: usize) -> String {
        if self == Self::Vietnamese {
            format!("Ấn bản này cung cấp {count} số trang")
        } else {
            format!("This edition supplies {count} page numbers")
        }
    }

    pub(super) fn publisher_page_position(self, chapter: usize, page: Option<usize>) -> String {
        match (self, page) {
            (Self::Vietnamese, Some(page)) => format!("Chương {chapter} · Trang màn hình {page}"),
            (Self::Vietnamese, None) => format!("Chương {chapter}"),
            (_, Some(page)) => format!("Chapter {chapter} · Screen page {page}"),
            (_, None) => format!("Chapter {chapter}"),
        }
    }

    pub(super) fn reading_guide_fixed<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) =
                READING_GUIDE_LABELS.iter().find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    /// Localize only generated guide-value units, preserving numeric values.
    pub(super) fn reading_guide_text(self, english: &str) -> String {
        if self == Self::Vietnamese {
            let digits = |s: &str| !s.is_empty() && s.bytes().all(|byte| byte.is_ascii_digit());
            if let Some(number) = english.strip_suffix(" of 10")
                && digits(number)
            {
                return format!("{number} trên 10");
            }
            if let Some(number) = english.strip_suffix(" pages")
                && digits(number)
            {
                return format!("{number} trang");
            }
        }
        self.reading_guide_fixed(english).to_owned()
    }

    pub(super) fn reading_guide_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = READING_GUIDE_LABELS
                .iter()
                .find(|(_, value)| *value == label)
            {
                return english;
            }
        }
        label
    }

    pub(super) fn bottom_typography_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = BOTTOM_TYPOGRAPHY_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn bottom_typography_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = BOTTOM_TYPOGRAPHY_LABELS
                .iter()
                .find(|(_, value)| *value == label)
            {
                return english;
            }
        }
        label
    }

    pub(super) fn top_settings_detail<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = TOP_SETTINGS_DETAIL_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn reading_profile_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = READING_PROFILE_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn hidden_flow_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) =
                HIDDEN_FLOW_LABELS.iter().find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn book_archive_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) =
                BOOK_ARCHIVE_LABELS.iter().find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn book_shortcut_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) =
                BOOK_SHORTCUT_LABELS.iter().find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn footnote_font_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) =
                FOOTNOTE_FONT_LABELS.iter().find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn metadata_settings_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = METADATA_SETTINGS_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn metadata_settings_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = METADATA_SETTINGS_LABELS
                .iter()
                .find(|(_, value)| *value == label)
            {
                return english;
            }
        }
        label
    }

    pub(super) fn vocabulary_settings_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = VOCABULARY_SETTINGS_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn library_export_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = LIBRARY_EXPORT_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn reading_backup_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = READING_BACKUP_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn export_settings_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = EXPORT_SETTINGS_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn history_reset_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) =
                HISTORY_RESET_LABELS.iter().find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn history_import_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = HISTORY_IMPORT_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn history_month_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) =
                HISTORY_MONTH_LABELS.iter().find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn history_timeline_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = HISTORY_TIMELINE_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn history_progress_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = HISTORY_PROGRESS_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn history_report_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = HISTORY_REPORT_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn history_summary_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = HISTORY_SUMMARY_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn history_settings_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = HISTORY_SETTINGS_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn history_calendar_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = HISTORY_CALENDAR_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn annotation_exchange_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = ANNOTATION_EXCHANGE_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn annotation_exchange_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = ANNOTATION_EXCHANGE_LABELS
                .iter()
                .find(|(_, value)| *value == label)
            {
                return english;
            }
        }
        label
    }

    pub(super) fn typography_settings_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = TYPOGRAPHY_SETTINGS_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn typography_settings_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = TYPOGRAPHY_SETTINGS_LABELS
                .iter()
                .find(|(_, value)| *value == label)
            {
                return english;
            }
        }
        label
    }

    pub(super) fn scrolling_settings_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = SCROLLING_SETTINGS_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn scrolling_settings_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = SCROLLING_SETTINGS_LABELS
                .iter()
                .find(|(_, value)| *value == label)
            {
                return english;
            }
        }
        label
    }

    pub(super) fn navigation_settings_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = NAVIGATION_SETTINGS_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn navigation_settings_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = NAVIGATION_SETTINGS_LABELS
                .iter()
                .find(|(_, value)| *value == label)
            {
                return english;
            }
        }
        label
    }

    pub(super) fn page_turn_settings_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = PAGE_TURN_SETTINGS_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn page_turn_settings_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = PAGE_TURN_SETTINGS_LABELS
                .iter()
                .find(|(_, value)| *value == label)
            {
                return english;
            }
        }
        label
    }

    pub(super) fn link_settings_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) =
                LINK_SETTINGS_LABELS.iter().find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn link_settings_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = LINK_SETTINGS_LABELS
                .iter()
                .find(|(_, value)| *value == label)
            {
                return english;
            }
        }
        label
    }

    pub(super) fn notification_settings_text<'a>(self, english: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((_, vietnamese)) = NOTIFICATION_SETTINGS_LABELS
                .iter()
                .find(|(key, _)| *key == english)
            {
                return vietnamese;
            }
        }
        english
    }

    pub(super) fn notification_settings_english<'a>(self, label: &'a str) -> &'a str {
        if self == Self::Vietnamese {
            if let Some((english, _)) = NOTIFICATION_SETTINGS_LABELS
                .iter()
                .find(|(_, value)| *value == label)
            {
                return english;
            }
        }
        label
    }

    pub(super) fn notification_source_count(self, enabled: usize, total: usize) -> String {
        if self == Self::Vietnamese {
            format!("Đã bật {enabled} trên {total} nguồn")
        } else {
            format!("{enabled} of {total} sources on")
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
