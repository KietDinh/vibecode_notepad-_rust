//! Command identifiers, the main menu tree (mirrors Notepad++), and keyboard accelerators.

use crate::lang::SPECS;
use crate::win::w;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

#[repr(u16)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cmd {
    New = 1000,
    Open,
    OpenFolder,
    OpenCmd,
    OpenExplorerSelect,
    Reload,
    Save,
    SaveAs,
    SaveCopy,
    SaveAll,
    Rename,
    Close,
    CloseAll,
    CloseOthers,
    CloseLeft,
    CloseRight,
    CloseUnchanged,
    DeleteFile,
    Print,
    RestoreClosed,
    OpenAllRecent,
    EmptyRecent,
    Exit,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Delete,
    SelectAll,
    BeginEndSelect,
    InsertDateShort,
    InsertDateLong,
    CopyFullPath,
    CopyFileName,
    CopyDirPath,
    Indent,
    Outdent,
    Upper,
    Lower,
    Proper,
    Sentence,
    Invert,
    RandomCase,
    DupLine,
    RemoveDupLines,
    RemoveConsecDup,
    SplitLines,
    JoinLines,
    MoveUp,
    MoveDown,
    RemoveEmptyLines,
    RemoveEmptyLinesWs,
    BlankAbove,
    BlankBelow,
    ReverseLines,
    SortAsc,
    SortDesc,
    SortAscCi,
    SortDescCi,
    SortNumAsc,
    SortNumDesc,
    SortLenAsc,
    DeleteLine,
    Transpose,
    ToggleComment,
    LineComment,
    LineUncomment,
    BlockComment,
    BlockUncomment,
    CompleteWord,
    EolCrlf,
    EolLf,
    EolCr,
    TrimTrailing,
    TrimLeading,
    TrimBoth,
    EolToSpace,
    TrimAndEolToSpace,
    TabToSpace,
    SpaceToTabAll,
    SpaceToTabLeading,
    ColumnEditor,
    OpenSelectedFile,
    SearchInternet,
    ReadOnly,
    Find,
    FindInFiles,
    FindNext,
    FindPrev,
    SelectFindNext,
    SelectFindPrev,
    Replace,
    Mark,
    SearchResults,
    NextResult,
    PrevResult,
    Goto,
    GotoBrace,
    SelectBrace,
    Style1,
    Style2,
    Style3,
    Style4,
    Style5,
    ClearStyle1,
    ClearStyle2,
    ClearStyle3,
    ClearStyle4,
    ClearStyle5,
    ClearAllStyles,
    ToggleBookmark,
    NextBookmark,
    PrevBookmark,
    ClearBookmarks,
    CutBookmarked,
    CopyBookmarked,
    RemoveBookmarked,
    RemoveUnbookmarked,
    InverseBookmark,
    AlwaysOnTop,
    FullScreen,
    PostIt,
    ShowWs,
    ShowEol,
    ShowAll,
    ShowIndentGuide,
    ZoomIn,
    ZoomOut,
    ZoomRestore,
    WordWrap,
    FoldAll,
    UnfoldAll,
    FoldCurrent,
    UnfoldCurrent,
    FoldLevel1,
    FoldLevel2,
    FoldLevel3,
    FoldLevel4,
    FoldLevel5,
    FoldLevel6,
    FoldLevel7,
    FoldLevel8,
    UnfoldLevel1,
    UnfoldLevel2,
    UnfoldLevel3,
    UnfoldLevel4,
    UnfoldLevel5,
    UnfoldLevel6,
    UnfoldLevel7,
    UnfoldLevel8,
    Summary,
    DocMap,
    FuncList,
    NextTab,
    PrevTab,
    MoveTabFwd,
    MoveTabBack,
    Tab1,
    Tab2,
    Tab3,
    Tab4,
    Tab5,
    Tab6,
    Tab7,
    Tab8,
    Tab9,
    Monitoring,
    EncAnsi,
    EncUtf8,
    EncUtf8Bom,
    EncUtf16Be,
    EncUtf16Le,
    ConvAnsi,
    ConvUtf8,
    ConvUtf8Bom,
    ConvUtf16Be,
    ConvUtf16Le,
    Preferences,
    StyleConfigurator,
    ShortcutMapper,
    ToggleToolbar,
    ToggleStatusbar,
    ToggleTabbar,
    HashMd5,
    HashSha256,
    MacroRecord,
    MacroStop,
    MacroPlay,
    MacroRunMulti,
    TrimSave,
    Run,
    LaunchBrowser,
    Wikipedia,
    Base64Enc,
    Base64Dec,
    UrlEnc,
    UrlDec,
    AsciiToHex,
    HexToAscii,
    WindowsList,
    SortTabsName,
    SortTabsPath,
    Homepage,
    UserManual,
    DebugInfo,
    About,
    CmdLineHelp,
    Escape,
    Last,
}

impl Cmd {
    pub fn from_id(v: u16) -> Option<Cmd> {
        if (Cmd::New as u16..Cmd::Last as u16).contains(&v) {
            // SAFETY: Cmd is repr(u16) with contiguous discriminants from New to Last.
            Some(unsafe { std::mem::transmute::<u16, Cmd>(v) })
        } else {
            None
        }
    }
}

pub const ID_RECENT: u16 = 3000;
pub const ID_LANG: u16 = 4000;
pub const ID_WINDOW: u16 = 6000;

pub enum M {
    I(&'static str, Cmd, &'static str),
    Sep,
    Sub(&'static str, &'static [M]),
    Recent,
    Languages,
    Windows,
}

use Cmd::*;
use M::*;

pub static MENU: &[M] = &[
    Sub(
        "&File",
        &[
            I("&New", New, "Ctrl+N"),
            I("&Open...", Open, "Ctrl+O"),
            Sub(
                "Open Containing &Folder",
                &[I("&Explorer", OpenFolder, ""), I("c&md", OpenCmd, "")],
            ),
            I("Re&load from Disk", Reload, ""),
            I("&Save", Save, "Ctrl+S"),
            I("Save &As...", SaveAs, "Ctrl+Alt+S"),
            I("Save a Copy As...", SaveCopy, ""),
            I("Sa&ve All", SaveAll, "Ctrl+Shift+S"),
            I("&Rename...", Rename, ""),
            I("&Close", Close, "Ctrl+W"),
            I("Clos&e All", CloseAll, "Ctrl+Shift+W"),
            Sub(
                "Close &Multiple Documents",
                &[
                    I("Close All BUT Current Document", CloseOthers, ""),
                    I("Close All to the Left", CloseLeft, ""),
                    I("Close All to the Right", CloseRight, ""),
                    I("Close All Unchanged", CloseUnchanged, ""),
                ],
            ),
            I("Move to Recycle &Bin", DeleteFile, ""),
            Sep,
            I("&Print...", Print, "Ctrl+P"),
            Sep,
            I("Restore Recent Closed File", RestoreClosed, "Ctrl+Shift+T"),
            Recent,
            I("Open All Recent Files", OpenAllRecent, ""),
            I("Empty Recent Files List", EmptyRecent, ""),
            Sep,
            I("E&xit", Exit, "Alt+F4"),
        ],
    ),
    Sub(
        "&Edit",
        &[
            I("&Undo", Undo, "Ctrl+Z"),
            I("&Redo", Redo, "Ctrl+Y"),
            Sep,
            I("Cu&t", Cut, "Ctrl+X"),
            I("&Copy", Copy, "Ctrl+C"),
            I("&Paste", Paste, "Ctrl+V"),
            I("&Delete", Delete, "Del"),
            I("Select A&ll", SelectAll, "Ctrl+A"),
            I("Begin/End &Select", BeginEndSelect, ""),
            Sep,
            Sub("Insert", &[I("Date Time (short)", InsertDateShort, ""), I("Date Time (long)", InsertDateLong, "")]),
            Sub(
                "Cop&y to Clipboard",
                &[
                    I("Copy Current Full File Path", CopyFullPath, ""),
                    I("Copy Current Filename", CopyFileName, ""),
                    I("Copy Current Dir. Path", CopyDirPath, ""),
                ],
            ),
            Sub("&Indent", &[I("Increase Line Indent", Indent, "Tab"), I("Decrease Line Indent", Outdent, "Shift+Tab")]),
            Sub(
                "Con&vert Case to",
                &[
                    I("&UPPERCASE", Upper, "Ctrl+Shift+U"),
                    I("&lowercase", Lower, "Ctrl+U"),
                    I("&Proper Case", Proper, "Alt+U"),
                    I("&Sentence case", Sentence, "Ctrl+Alt+U"),
                    I("&iNVERT cASE", Invert, "Alt+Shift+U"),
                    I("&ranDOm CasE", RandomCase, "Ctrl+Alt+Shift+U"),
                ],
            ),
            Sub(
                "&Line Operations",
                &[
                    I("Duplicate Current Line", DupLine, "Ctrl+D"),
                    I("Remove Duplicate Lines", RemoveDupLines, ""),
                    I("Remove Consecutive Duplicate Lines", RemoveConsecDup, ""),
                    I("Split Lines", SplitLines, "Ctrl+I"),
                    I("Join Lines", JoinLines, "Ctrl+J"),
                    I("Move Up Current Line", MoveUp, "Ctrl+Shift+Up"),
                    I("Move Down Current Line", MoveDown, "Ctrl+Shift+Down"),
                    I("Remove Empty Lines", RemoveEmptyLines, ""),
                    I("Remove Empty Lines (Containing Blank characters)", RemoveEmptyLinesWs, ""),
                    I("Insert Blank Line Above Current", BlankAbove, "Ctrl+Alt+Enter"),
                    I("Insert Blank Line Below Current", BlankBelow, "Ctrl+Alt+Shift+Enter"),
                    I("Reverse Line Order", ReverseLines, ""),
                    Sep,
                    I("Sort Lines Lexicographically Ascending", SortAsc, ""),
                    I("Sort Lines Lexicographically Descending", SortDesc, ""),
                    I("Sort Lines Lex. Ascending Ignoring Case", SortAscCi, ""),
                    I("Sort Lines Lex. Descending Ignoring Case", SortDescCi, ""),
                    I("Sort Lines As Integers Ascending", SortNumAsc, ""),
                    I("Sort Lines As Integers Descending", SortNumDesc, ""),
                    I("Sort Lines By Length Ascending", SortLenAsc, ""),
                    Sep,
                    I("Delete Current Line", DeleteLine, "Ctrl+Shift+L"),
                    I("Transpose Current Line", Transpose, "Ctrl+T"),
                ],
            ),
            Sub(
                "Comment/Uncomment",
                &[
                    I("Toggle Single Line Comment", ToggleComment, "Ctrl+Q"),
                    I("Single Line Comment", LineComment, "Ctrl+K"),
                    I("Single Line Uncomment", LineUncomment, "Ctrl+Shift+K"),
                    I("Block Comment", BlockComment, "Ctrl+Shift+Q"),
                    I("Block Uncomment", BlockUncomment, ""),
                ],
            ),
            Sub("Auto-Completion", &[I("Word Completion", CompleteWord, "Ctrl+Enter")]),
            Sub(
                "EOL Conversion",
                &[I("Windows (CR LF)", EolCrlf, ""), I("Unix (LF)", EolLf, ""), I("Macintosh (CR)", EolCr, "")],
            ),
            Sub(
                "Blank Operations",
                &[
                    I("Trim Trailing Space", TrimTrailing, ""),
                    I("Trim Leading Space", TrimLeading, ""),
                    I("Trim Leading and Trailing Space", TrimBoth, ""),
                    I("EOL to Space", EolToSpace, ""),
                    I("Trim both and EOL to Space", TrimAndEolToSpace, ""),
                    Sep,
                    I("TAB to Space", TabToSpace, ""),
                    I("Space to TAB (All)", SpaceToTabAll, ""),
                    I("Space to TAB (Leading)", SpaceToTabLeading, ""),
                ],
            ),
            Sub(
                "On Selection",
                &[I("Open File", OpenSelectedFile, ""), I("Search on Internet", SearchInternet, "")],
            ),
            Sep,
            I("Column Editor...", ColumnEditor, "Alt+C"),
            Sep,
            I("Set Read-Only", ReadOnly, ""),
        ],
    ),
    Sub(
        "&Search",
        &[
            I("&Find...", Find, "Ctrl+F"),
            I("Find in Files...", FindInFiles, "Ctrl+Shift+F"),
            I("Find &Next", FindNext, "F3"),
            I("Find &Previous", FindPrev, "Shift+F3"),
            I("Select and Find Next", SelectFindNext, "Ctrl+F3"),
            I("Select and Find Previous", SelectFindPrev, "Ctrl+Shift+F3"),
            I("&Replace...", Replace, "Ctrl+H"),
            I("Search Results Window", SearchResults, "F7"),
            I("Next Search Result", NextResult, "F4"),
            I("Previous Search Result", PrevResult, "Shift+F4"),
            I("&Go to...", Goto, "Ctrl+G"),
            I("Go to Matching &Brace", GotoBrace, "Ctrl+B"),
            I("Select All In-between {} [] or ()", SelectBrace, "Ctrl+Alt+B"),
            I("Mar&k...", Mark, "Ctrl+M"),
            Sep,
            Sub(
                "Style All Occurrences of Token",
                &[
                    I("Using 1st Style", Style1, ""),
                    I("Using 2nd Style", Style2, ""),
                    I("Using 3rd Style", Style3, ""),
                    I("Using 4th Style", Style4, ""),
                    I("Using 5th Style", Style5, ""),
                ],
            ),
            Sub(
                "Clear Style",
                &[
                    I("Clear 1st Style", ClearStyle1, ""),
                    I("Clear 2nd Style", ClearStyle2, ""),
                    I("Clear 3rd Style", ClearStyle3, ""),
                    I("Clear 4th Style", ClearStyle4, ""),
                    I("Clear 5th Style", ClearStyle5, ""),
                    I("Clear all Styles", ClearAllStyles, ""),
                ],
            ),
            Sep,
            Sub(
                "Bookmark",
                &[
                    I("Toggle Bookmark", ToggleBookmark, "Ctrl+F2"),
                    I("Next Bookmark", NextBookmark, "F2"),
                    I("Previous Bookmark", PrevBookmark, "Shift+F2"),
                    I("Clear All Bookmarks", ClearBookmarks, ""),
                    I("Cut Bookmarked Lines", CutBookmarked, ""),
                    I("Copy Bookmarked Lines", CopyBookmarked, ""),
                    I("Remove Bookmarked Lines", RemoveBookmarked, ""),
                    I("Remove Non-Bookmarked Lines", RemoveUnbookmarked, ""),
                    I("Inverse Bookmark", InverseBookmark, ""),
                ],
            ),
        ],
    ),
    Sub(
        "&View",
        &[
            I("Always on Top", AlwaysOnTop, ""),
            I("Toggle Full Screen Mode", FullScreen, "F11"),
            I("Post-It", PostIt, "F12"),
            Sep,
            Sub(
                "Show Symbol",
                &[
                    I("Show Space and Tab", ShowWs, ""),
                    I("Show End of Line", ShowEol, ""),
                    I("Show All Characters", ShowAll, ""),
                    Sep,
                    I("Show Indent Guide", ShowIndentGuide, ""),
                ],
            ),
            Sub(
                "Zoom",
                &[I("Zoom &In (Ctrl+Mouse Wheel Up)", ZoomIn, "Ctrl+Num +"), I("Zoom &Out (Ctrl+Mouse Wheel Down)", ZoomOut, "Ctrl+Num -"), I("Restore Default Zoom", ZoomRestore, "Ctrl+Num /")],
            ),
            Sub(
                "Tab",
                &[
                    I("1st Tab", Tab1, "Ctrl+1"),
                    I("2nd Tab", Tab2, "Ctrl+2"),
                    I("3rd Tab", Tab3, "Ctrl+3"),
                    I("4th Tab", Tab4, "Ctrl+4"),
                    I("5th Tab", Tab5, "Ctrl+5"),
                    I("6th Tab", Tab6, "Ctrl+6"),
                    I("7th Tab", Tab7, "Ctrl+7"),
                    I("8th Tab", Tab8, "Ctrl+8"),
                    I("9th Tab", Tab9, "Ctrl+9"),
                    Sep,
                    I("Next Tab", NextTab, "Ctrl+PgDn"),
                    I("Previous Tab", PrevTab, "Ctrl+PgUp"),
                    Sep,
                    I("Move Tab Forward", MoveTabFwd, "Ctrl+Shift+PgDn"),
                    I("Move Tab Backward", MoveTabBack, "Ctrl+Shift+PgUp"),
                ],
            ),
            I("Word wrap", WordWrap, ""),
            Sep,
            I("Fold All", FoldAll, "Alt+0"),
            I("Unfold All", UnfoldAll, "Alt+Shift+0"),
            I("Fold Current Level", FoldCurrent, "Ctrl+Alt+F"),
            I("Unfold Current Level", UnfoldCurrent, "Ctrl+Alt+Shift+F"),
            Sub(
                "Fold Level",
                &[
                    I("1", FoldLevel1, "Alt+1"),
                    I("2", FoldLevel2, "Alt+2"),
                    I("3", FoldLevel3, "Alt+3"),
                    I("4", FoldLevel4, "Alt+4"),
                    I("5", FoldLevel5, "Alt+5"),
                    I("6", FoldLevel6, "Alt+6"),
                    I("7", FoldLevel7, "Alt+7"),
                    I("8", FoldLevel8, "Alt+8"),
                ],
            ),
            Sub(
                "Unfold Level",
                &[
                    I("1", UnfoldLevel1, "Alt+Shift+1"),
                    I("2", UnfoldLevel2, "Alt+Shift+2"),
                    I("3", UnfoldLevel3, "Alt+Shift+3"),
                    I("4", UnfoldLevel4, "Alt+Shift+4"),
                    I("5", UnfoldLevel5, "Alt+Shift+5"),
                    I("6", UnfoldLevel6, "Alt+Shift+6"),
                    I("7", UnfoldLevel7, "Alt+Shift+7"),
                    I("8", UnfoldLevel8, "Alt+Shift+8"),
                ],
            ),
            Sep,
            I("Summary...", Summary, ""),
            Sep,
            I("Document Map", DocMap, ""),
            I("Function List", FuncList, ""),
            Sep,
            I("Monitoring (tail -f)", Monitoring, ""),
        ],
    ),
    Sub(
        "E&ncoding",
        &[
            I("ANSI", EncAnsi, ""),
            I("UTF-8", EncUtf8, ""),
            I("UTF-8-BOM", EncUtf8Bom, ""),
            I("UTF-16 BE BOM", EncUtf16Be, ""),
            I("UTF-16 LE BOM", EncUtf16Le, ""),
            Sep,
            I("Convert to ANSI", ConvAnsi, ""),
            I("Convert to UTF-8", ConvUtf8, ""),
            I("Convert to UTF-8-BOM", ConvUtf8Bom, ""),
            I("Convert to UTF-16 BE BOM", ConvUtf16Be, ""),
            I("Convert to UTF-16 LE BOM", ConvUtf16Le, ""),
        ],
    ),
    Sub("&Language", &[Languages]),
    Sub(
        "Se&ttings",
        &[
            I("Preferences...", Preferences, ""),
            I("Style Configurator...", StyleConfigurator, ""),
            I("Shortcut Mapper...", ShortcutMapper, ""),
            Sep,
            I("Show Toolbar", ToggleToolbar, ""),
            I("Show Tab Bar", ToggleTabbar, ""),
            I("Show Status Bar", ToggleStatusbar, ""),
        ],
    ),
    Sub(
        "T&ools",
        &[
            Sub("MD5", &[I("Generate from selection into clipboard", HashMd5, "")]),
            Sub("SHA-256", &[I("Generate from selection into clipboard", HashSha256, "")]),
        ],
    ),
    Sub(
        "&Macro",
        &[
            I("Start Recording", MacroRecord, "Ctrl+Shift+R"),
            I("Stop Recording", MacroStop, ""),
            I("Playback", MacroPlay, "Ctrl+Shift+P"),
            I("Run a Macro Multiple Times...", MacroRunMulti, ""),
            Sep,
            I("Trim Trailing Space and Save", TrimSave, "Alt+Shift+S"),
        ],
    ),
    Sub(
        "&Run",
        &[
            I("&Run...", Run, "F5"),
            Sep,
            I("Launch in Browser", LaunchBrowser, ""),
            I("Wikipedia Search", Wikipedia, ""),
            I("Open file in Explorer", OpenExplorerSelect, ""),
        ],
    ),
    Sub(
        "&Plugins",
        &[
            Sub(
                "MIME Tools",
                &[I("Base64 Encode", Base64Enc, ""), I("Base64 Decode", Base64Dec, ""), Sep, I("URL Encode", UrlEnc, ""), I("URL Decode", UrlDec, "")],
            ),
            Sub("Converter", &[I("ASCII -> HEX", AsciiToHex, ""), I("HEX -> ASCII", HexToAscii, "")]),
        ],
    ),
    Sub(
        "&Window",
        &[
            Sub("Sort By", &[I("Name A to Z", SortTabsName, ""), I("Path A to Z", SortTabsPath, "")]),
            Sep,
            Windows,
            Sep,
            I("&Windows...", WindowsList, ""),
        ],
    ),
    Sub(
        "&?",
        &[
            I("Command Line Arguments...", CmdLineHelp, ""),
            Sep,
            I("Notepad++ Home", Homepage, ""),
            I("Online User Manual", UserManual, "Shift+F1"),
            Sep,
            I("Debug Info...", DebugInfo, ""),
            I("About Notepad++ RS", About, "F1"),
        ],
    ),
];

pub struct Built {
    pub bar: HMENU,
    pub recent: Vec<(HMENU, u32)>,
    pub languages: HMENU,
    pub windows: Vec<(HMENU, u32)>,
}

fn label(text: &str, sc: &str) -> Vec<u16> {
    if sc.is_empty() {
        w(text)
    } else {
        w(&format!("{text}\t{sc}"))
    }
}

pub fn build() -> Built {
    unsafe {
        let bar = CreateMenu();
        let mut built = Built { bar, recent: Vec::new(), languages: std::ptr::null_mut(), windows: Vec::new() };
        for m in MENU {
            add(bar, m, &mut built);
        }
        built
    }
}

unsafe fn add(parent: HMENU, m: &M, b: &mut Built) {
    match m {
        I(t, c, sc) => {
            let l = label(t, sc);
            AppendMenuW(parent, MF_STRING, *c as usize, l.as_ptr());
        }
        Sep => {
            AppendMenuW(parent, MF_SEPARATOR, 0, std::ptr::null());
        }
        Sub(t, items) => {
            let sub = CreatePopupMenu();
            for it in items.iter() {
                add(sub, it, b);
            }
            let l = w(t);
            AppendMenuW(parent, MF_POPUP, sub as usize, l.as_ptr());
            if matches!(items.first(), Some(Languages)) {
                b.languages = sub;
            }
        }
        Recent => {
            let n = GetMenuItemCount(parent) as u32;
            b.recent.push((parent, n));
        }
        Languages => {
            build_languages(parent);
        }
        Windows => {
            let n = GetMenuItemCount(parent) as u32;
            b.windows.push((parent, n));
        }
    }
}

unsafe fn build_languages(menu: HMENU) {
    let l = w("None (Normal Text)");
    AppendMenuW(menu, MF_STRING, ID_LANG as usize, l.as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
    let mut letters: Vec<char> = SPECS.iter().skip(1).filter(|s| !s.exts.is_empty()).map(|s| s.name.chars().next().unwrap().to_ascii_uppercase()).collect();
    letters.sort();
    letters.dedup();
    for letter in letters {
        let sub = CreatePopupMenu();
        let mut items: Vec<(usize, &str)> = SPECS
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, s)| !s.exts.is_empty() && s.name.chars().next().unwrap().to_ascii_uppercase() == letter)
            .map(|(i, s)| (i, s.name))
            .collect();
        items.sort_by_key(|x| x.1.to_lowercase());
        for (i, name) in items {
            let l = w(name);
            AppendMenuW(sub, MF_STRING, ID_LANG as usize + i, l.as_ptr());
        }
        let l = w(&letter.to_string());
        AppendMenuW(menu, MF_POPUP, sub as usize, l.as_ptr());
    }
}

/// Replace dynamic items (recent files / window list) in a popup.
pub fn set_dynamic(menu: HMENU, at: u32, base: u16, old_count: usize, items: &[String]) -> usize {
    unsafe {
        for _ in 0..old_count {
            DeleteMenu(menu, at, MF_BYPOSITION);
        }
        for (i, s) in items.iter().enumerate() {
            let t = if i < 9 { format!("&{}: {}", i + 1, s) } else { format!("{}: {}", i + 1, s) };
            let l = w(&t.replace('&', "&&").replacen("&&", "&", 1));
            InsertMenuW(menu, at + i as u32, MF_BYPOSITION | MF_STRING, base as usize + i, l.as_ptr());
        }
    }
    items.len()
}

/// Parse "Ctrl+Shift+F3" into an accelerator entry.
fn parse_accel(sc: &str, cmd: u16) -> Option<ACCEL> {
    if sc.is_empty() {
        return None;
    }
    let mut flags = FVIRTKEY;
    let mut key: Option<u16> = None;
    for part in sc.split('+') {
        let p = part.trim();
        match p {
            "Ctrl" => flags |= FCONTROL,
            "Shift" => flags |= FSHIFT,
            "Alt" => flags |= FALT,
            "" => {}
            _ => {
                key = Some(match p {
                    "Tab" | "Del" => return None, // handled by the editor itself
                    "Enter" => VK_RETURN,
                    "Up" => VK_UP,
                    "Down" => VK_DOWN,
                    "PgUp" => VK_PRIOR,
                    "PgDn" => VK_NEXT,
                    "F4" if flags == FVIRTKEY | FALT => return None,
                    _ if p.starts_with("Num ") => match &p[4..] {
                        "+" | "" => VK_ADD,
                        "-" => VK_SUBTRACT,
                        "/" => VK_DIVIDE,
                        _ => return None,
                    },
                    _ if p.len() > 1 && p.starts_with('F') => VK_F1 + p[1..].parse::<u16>().ok()? - 1,
                    _ if p.len() == 1 => p.as_bytes()[0].to_ascii_uppercase() as u16,
                    _ => return None,
                });
            }
        }
    }
    // "Num +" is split by '+': handle "Ctrl+Num +" explicitly
    if sc.ends_with("Num +") {
        key = Some(VK_ADD);
    }
    key.map(|k| ACCEL { fVirt: flags as u8, key: k, cmd })
}

fn collect(items: &[M], out: &mut Vec<ACCEL>) {
    for m in items {
        match m {
            I(_, c, sc) => {
                if let Some(a) = parse_accel(sc, *c as u16) {
                    out.push(a);
                }
            }
            Sub(_, sub) => collect(sub, out),
            _ => {}
        }
    }
}

pub fn accelerators() -> HACCEL {
    let mut v = Vec::new();
    collect(MENU, &mut v);
    let extra: &[(u8, u16, Cmd)] = &[
        ((FVIRTKEY | FCONTROL) as u8, VK_TAB, NextTab),
        ((FVIRTKEY | FCONTROL | FSHIFT) as u8, VK_TAB, PrevTab),
        ((FVIRTKEY | FCONTROL) as u8, VK_OEM_PLUS, ZoomIn),
        ((FVIRTKEY | FCONTROL) as u8, VK_OEM_MINUS, ZoomOut),
        ((FVIRTKEY | FCONTROL) as u8, VK_INSERT, Copy),
        ((FVIRTKEY | FSHIFT) as u8, VK_INSERT, Paste),
        ((FVIRTKEY | FSHIFT) as u8, VK_DELETE, Cut),
        ((FVIRTKEY | FALT) as u8, VK_BACK, Undo),
        ((FVIRTKEY | FCONTROL | FSHIFT) as u8, b'Z' as u16, Redo),
        ((FVIRTKEY | FCONTROL) as u8, VK_F4, Close),
    ];
    for (f, k, c) in extra {
        v.push(ACCEL { fVirt: *f, key: *k, cmd: *c as u16 });
    }
    unsafe { CreateAcceleratorTableW(v.as_ptr(), v.len() as i32) }
}

/// All shortcut strings, used by the Shortcut Mapper listing.
pub fn shortcut_list() -> Vec<(String, String)> {
    fn walk(items: &[M], prefix: &str, out: &mut Vec<(String, String)>) {
        for m in items {
            match m {
                I(t, _, sc) if !sc.is_empty() => out.push((format!("{prefix}{}", t.replace('&', "")), sc.to_string())),
                Sub(t, sub) => walk(sub, &format!("{prefix}{} > ", t.replace('&', "")), out),
                _ => {}
            }
        }
    }
    let mut v = Vec::new();
    walk(MENU, "", &mut v);
    v
}
