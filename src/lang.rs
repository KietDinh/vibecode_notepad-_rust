//! Language definitions: names, extensions, comment tokens and keyword sets.

use std::collections::HashSet;
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Lang {
    Text,
    Asm,
    Batch,
    C,
    Cpp,
    CSharp,
    CMake,
    Css,
    Diff,
    Go,
    Haskell,
    Html,
    Ini,
    Java,
    JavaScript,
    Json,
    Kotlin,
    Lua,
    Makefile,
    Markdown,
    ObjC,
    Pascal,
    Perl,
    Php,
    PowerShell,
    Properties,
    Python,
    R,
    Ruby,
    Rust,
    Shell,
    Sql,
    Swift,
    Tex,
    Toml,
    TypeScript,
    VB,
    Xml,
    Yaml,
    SearchResult,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LexKind {
    Plain,
    CLike,
    Markup,
    Ini,
    Diff,
    Markdown,
    Yaml,
    Batch,
    SearchResult,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fold {
    None,
    Brace,
    Indent,
    Tag,
    Section,
}

pub struct Spec {
    pub lang: Lang,
    pub name: &'static str,
    pub desc: &'static str,
    pub exts: &'static [&'static str],
    pub kind: LexKind,
    pub fold: Fold,
    pub line_comment: &'static [&'static str],
    pub block_comment: Option<(&'static str, &'static str)>,
    pub keywords: &'static str,
    pub types: &'static str,
    pub builtins: &'static str,
    pub nocase: bool,
    pub quotes: &'static str,
    /// Quotes whose strings may span lines.
    pub ml_quotes: &'static str,
    /// `'` introduces a char literal (C-like).
    pub char_lit: bool,
    pub preproc: bool,
    pub triple: bool,
    pub rust: bool,
    pub var_prefix: Option<char>,
    pub decorator: bool,
    pub json: bool,
    pub css: bool,
    pub fold_square: bool,
}

const BASE: Spec = Spec {
    lang: Lang::Text,
    name: "",
    desc: "",
    exts: &[],
    kind: LexKind::CLike,
    fold: Fold::Brace,
    line_comment: &["//"],
    block_comment: Some(("/*", "*/")),
    keywords: "",
    types: "",
    builtins: "",
    nocase: false,
    quotes: "\"",
    ml_quotes: "",
    char_lit: true,
    preproc: false,
    triple: false,
    rust: false,
    var_prefix: None,
    decorator: false,
    json: false,
    css: false,
    fold_square: false,
};

const C_KW: &str = "break case const continue default do else enum extern for goto if inline register restrict return sizeof static struct switch typedef union volatile while _Alignas _Alignof _Atomic _Generic _Noreturn _Static_assert _Thread_local";
const C_TY: &str = "auto char double float int long short signed unsigned void bool _Bool _Complex size_t ssize_t int8_t int16_t int32_t int64_t uint8_t uint16_t uint32_t uint64_t intptr_t uintptr_t wchar_t FILE NULL";
const CPP_KW: &str = "alignas alignof and and_eq asm break case catch class co_await co_return co_yield concept const const_cast consteval constexpr constinit continue decltype default delete do dynamic_cast else enum explicit export extern false final for friend goto if inline mutable namespace new noexcept not not_eq nullptr operator or or_eq override private protected public register reinterpret_cast requires return sizeof static static_assert static_cast struct switch template this thread_local throw true try typedef typeid typename union using virtual volatile while xor xor_eq";
const CPP_TY: &str = "auto bool char char8_t char16_t char32_t double float int long short signed unsigned void wchar_t size_t string vector map set unordered_map unordered_set shared_ptr unique_ptr weak_ptr std int8_t int16_t int32_t int64_t uint8_t uint16_t uint32_t uint64_t nullptr_t";
const CS_KW: &str = "abstract as base break case catch checked class const continue default delegate do else enum event explicit extern false finally fixed for foreach goto if implicit in interface internal is lock namespace new null operator out override params private protected public readonly ref return sealed sizeof stackalloc static struct switch this throw true try typeof unchecked unsafe using virtual volatile while async await var yield get set add remove partial where when record init with nameof global";
const CS_TY: &str = "bool byte char decimal double dynamic float int long object sbyte short string uint ulong ushort void nint nuint String Object List Dictionary Task";
const JAVA_KW: &str = "abstract assert break case catch class const continue default do else enum extends final finally for goto if implements import instanceof interface native new package private protected public return static strictfp super switch synchronized this throw throws transient try volatile while true false null var record sealed permits yield";
const JAVA_TY: &str = "boolean byte char double float int long short void String Object Integer Long Double Float Boolean Character List Map Set ArrayList HashMap";
const JS_KW: &str = "async await break case catch class const continue debugger default delete do else export extends finally for from function if import in instanceof let new of return static super switch this throw try typeof var void while with yield true false null undefined NaN Infinity get set";
const JS_TY: &str = "Array Boolean Date Error Function JSON Map Math Number Object Promise Proxy RegExp Set String Symbol WeakMap WeakSet BigInt console window document globalThis";
const TS_KW: &str = "abstract any as async await boolean break case catch class const constructor continue declare default delete do else enum export extends false finally for from function get if implements import in infer instanceof interface is keyof let module namespace never new null number object of package private protected public readonly require return set static string super switch symbol this throw true try type typeof undefined unique unknown var void while with yield satisfies";
const GO_KW: &str = "break case chan const continue default defer else fallthrough for func go goto if import interface map package range return select struct switch type var true false nil iota";
const GO_TY: &str = "bool byte complex64 complex128 error float32 float64 int int8 int16 int32 int64 rune string uint uint8 uint16 uint32 uint64 uintptr any comparable append cap close complex copy delete imag len make new panic print println real recover";
const RUST_KW: &str = "as async await break const continue crate dyn else enum extern false fn for if impl in let loop match mod move mut pub ref return self Self static struct super trait true type union unsafe use where while yield macro_rules";
const RUST_TY: &str = "bool char f32 f64 i8 i16 i32 i64 i128 isize str u8 u16 u32 u64 u128 usize String Vec Option Some None Result Ok Err Box Rc Arc RefCell Cell HashMap HashSet BTreeMap BTreeSet";
const SWIFT_KW: &str = "associatedtype class deinit enum extension fileprivate func import init inout internal let open operator private protocol public rethrows static struct subscript typealias var break case continue default defer do else fallthrough for guard if in repeat return switch where while as catch false is nil super self Self throw throws true try async await some any";
const SWIFT_TY: &str = "Int Int8 Int16 Int32 Int64 UInt Float Double Bool String Character Array Dictionary Set Optional Void Any";
const KT_KW: &str = "as break class continue do else false for fun if in interface is null object package return super this throw true try typealias typeof val var when while by catch constructor delegate dynamic field file finally get import init param property receiver set setparam where abstract annotation companion const crossinline data enum expect external final infix inline inner internal lateinit noinline open operator out override private protected public reified sealed suspend tailrec vararg";
const KT_TY: &str = "Int Long Short Byte Double Float Boolean Char String Unit Any Nothing List Map Set Array MutableList MutableMap";
const PY_KW: &str = "False None True and as assert async await break class continue def del elif else except finally for from global if import in is lambda nonlocal not or pass raise return try while with yield match case";
const PY_BI: &str = "abs all any bin bool bytearray bytes callable chr classmethod compile complex delattr dict dir divmod enumerate eval exec filter float format frozenset getattr globals hasattr hash help hex id input int isinstance issubclass iter len list locals map max memoryview min next object oct open ord pow print property range repr reversed round set setattr slice sorted staticmethod str sum super tuple type vars zip self cls __init__ __name__ __main__";
const PHP_KW: &str = "abstract and array as break callable case catch class clone const continue declare default do echo else elseif empty enddeclare endfor endforeach endif endswitch endwhile eval exit extends final finally fn for foreach function global goto if implements include include_once instanceof insteadof interface isset list match namespace new or print private protected public readonly require require_once return static switch throw trait try unset use var while xor yield true false null";
const PERL_KW: &str = "my our local sub if elsif else unless while until for foreach do last next redo return use no require package BEGIN END and or not eq ne lt gt le ge cmp print printf open close die warn defined undef shift push pop split join keys values each exists delete chomp chop length substr sprintf sort map grep scalar ref bless";
const RUBY_KW: &str = "BEGIN END alias and begin break case class def defined? do else elsif end ensure false for if in module next nil not or redo rescue retry return self super then true undef unless until when while yield require attr_accessor attr_reader attr_writer puts print lambda proc";
const LUA_KW: &str = "and break do else elseif end false for function goto if in local nil not or repeat return then true until while";
const LUA_BI: &str = "assert collectgarbage dofile error getmetatable ipairs load loadfile next pairs pcall print rawequal rawget rawlen rawset require select setmetatable tonumber tostring type xpcall string table math io os coroutine utf8 debug";
const SQL_KW: &str = "add all alter and any as asc authorization backup begin between break browse bulk by cascade case check checkpoint close clustered coalesce collate column commit compute constraint contains continue convert create cross current current_date current_time current_timestamp current_user cursor database dbcc deallocate declare default delete deny desc disk distinct distributed double drop dump else end errlvl escape except exec execute exists exit external fetch file fillfactor for foreign freetext from full function goto grant group having holdlock identity if in index inner insert intersect into is join key kill left like limit lineno load merge national nocheck nonclustered not null nullif of off offsets on open option or order outer over percent pivot plan precision primary print proc procedure public raiserror read readtext reconfigure references replication restore restrict return revert revoke right rollback rowcount rule save schema select session_user set setuser shutdown some statistics system_user table tablesample textsize then to top tran transaction trigger truncate union unique unpivot update use user values varying view waitfor when where while with returning offset fetch next only rows row";
const SQL_TY: &str = "int integer bigint smallint tinyint bit decimal numeric money float real date datetime datetime2 time timestamp char varchar nchar nvarchar text ntext binary varbinary image uniqueidentifier xml boolean serial blob clob count sum avg min max";
const SH_KW: &str = "if then else elif fi case esac for select while until do done in function time coproc return exit break continue local export readonly declare typeset unset shift source alias echo printf read cd pwd test eval exec set trap wait kill true false";
const PS_KW: &str = "begin break catch class continue data define do dynamicparam else elseif end enum exit filter finally for foreach from function hidden if in param process return static switch throw trap try until using var while workflow parallel sequence inlinescript";
const BAT_KW: &str = "rem set if exist errorlevel for in do break call copy chcp cd chdir choice cls country ctty date del erase dir echo exit goto loadfix loadhigh mkdir md move path pause prompt rename ren rmdir rd shift time type ver verify vol com con lpt nul defined not equ neq lss leq gtr geq else setlocal endlocal pushd popd start title color assoc ftype findstr find xcopy robocopy tasklist taskkill where";
const VB_KW: &str = "addhandler addressof alias and andalso as boolean byref byte byval call case catch cbool cbyte cchar cdate cdbl cdec char cint class clng cobj const continue csbyte cshort csng cstr ctype cuint culng cushort date decimal declare default delegate dim directcast do double each else elseif end endif enum erase error event exit false finally for friend function get gettype global gosub goto handles if implements imports in inherits integer interface is isnot let lib like long loop me mod module mustinherit mustoverride mybase myclass namespace narrowing new next not nothing notinheritable notoverridable object of on operator option optional or orelse overloads overridable overrides paramarray partial private property protected public raiseevent readonly redim rem removehandler resume return sbyte select set shadows shared short single static step stop string structure sub synclock then throw to true try trycast typeof uinteger ulong ushort using variant wend when while widening with withevents writeonly xor";
const PAS_KW: &str = "and array as asm begin case class const constructor destructor div do downto else end except exports file finalization finally for function goto if implementation in inherited initialization inline interface is label library mod nil not object of or out packed procedure program property raise record repeat resourcestring set shl shr string then threadvar to try type unit until uses var while with xor private protected public published override virtual abstract overload true false integer boolean char real double byte word cardinal longint pointer";
const ASM_KW: &str = "mov add sub mul imul div idiv inc dec and or xor not neg shl shr sal sar rol ror cmp test jmp je jne jz jnz jg jge jl jle ja jae jb jbe call ret push pop lea nop int syscall loop cld std rep movsb movsw movsd stosb lodsb section global extern db dw dd dq resb resw resd equ times org bits";
const ASM_TY: &str = "eax ebx ecx edx esi edi esp ebp rax rbx rcx rdx rsi rdi rsp rbp r8 r9 r10 r11 r12 r13 r14 r15 ax bx cx dx al bl cl dl ah bh ch dh cs ds es fs gs ss byte word dword qword ptr";
const HS_KW: &str = "case class data default deriving do else foreign if import in infix infixl infixr instance let module newtype of then type where qualified as hiding forall mdo family role pattern";
const R_KW: &str = "if else repeat while function for in next break TRUE FALSE NULL Inf NaN NA NA_integer_ NA_real_ NA_character_ library require return invisible";
const CMAKE_KW: &str = "add_compile_definitions add_compile_options add_custom_command add_custom_target add_definitions add_dependencies add_executable add_library add_subdirectory add_test cmake_minimum_required configure_file else elseif endforeach endfunction endif endmacro endwhile file find_library find_package find_path foreach function get_filename_component if include include_directories install link_directories list macro message option project return set set_property set_target_properties string target_compile_definitions target_compile_features target_compile_options target_include_directories target_link_libraries target_sources unset while";
const CSS_KW: &str = "color background background-color background-image border border-radius bottom box-shadow display flex flex-direction font font-family font-size font-weight gap grid height justify-content align-items left line-height margin margin-top margin-bottom margin-left margin-right max-width min-width opacity overflow padding padding-top padding-bottom padding-left padding-right position right text-align text-decoration top transform transition width z-index cursor content outline visibility white-space";
const MAKE_KW: &str = "ifeq ifneq ifdef ifndef else endif include define endef export unexport override vpath";
const TOML_KW: &str = "true false";
const JSON_KW: &str = "true false null";
const TEX_KW: &str = "";
const OBJC_KW: &str = "@interface @implementation @end @property @synthesize @protocol @class @selector @encode @try @catch @finally @throw @autoreleasepool self super nil Nil YES NO id instancetype nonatomic atomic strong weak retain assign copy readonly readwrite break case const continue default do else enum extern for goto if inline return sizeof static struct switch typedef union volatile while";
const YAML_KW: &str = "true false yes no on off null True False Yes No On Off Null TRUE FALSE NULL";

pub static SPECS: &[Spec] = &[
    Spec { lang: Lang::Text, name: "Normal Text", desc: "Normal text file", exts: &["txt"], kind: LexKind::Plain, fold: Fold::None, line_comment: &[], block_comment: None, ..BASE },
    Spec { lang: Lang::Asm, name: "Assembly", desc: "Assembly language source file", exts: &["asm", "s", "nasm"], fold: Fold::None, line_comment: &[";"], block_comment: None, keywords: ASM_KW, types: ASM_TY, nocase: true, quotes: "\"'", char_lit: false, ..BASE },
    Spec { lang: Lang::Batch, name: "Batch", desc: "Batch file", exts: &["bat", "cmd", "nt"], kind: LexKind::Batch, fold: Fold::None, line_comment: &["REM", "::"], block_comment: None, keywords: BAT_KW, nocase: true, ..BASE },
    Spec { lang: Lang::C, name: "C", desc: "C source file", exts: &["c", "h"], keywords: C_KW, types: C_TY, preproc: true, ..BASE },
    Spec { lang: Lang::Cpp, name: "C++", desc: "C++ source file", exts: &["cpp", "cxx", "cc", "hpp", "hxx", "hh", "inl", "ino", "ipp", "tpp"], keywords: CPP_KW, types: CPP_TY, preproc: true, ..BASE },
    Spec { lang: Lang::CSharp, name: "C#", desc: "C# source file", exts: &["cs"], keywords: CS_KW, types: CS_TY, preproc: true, ml_quotes: "", ..BASE },
    Spec { lang: Lang::CMake, name: "CMake", desc: "CMake file", exts: &["cmake"], fold: Fold::None, line_comment: &["#"], block_comment: None, keywords: CMAKE_KW, nocase: true, char_lit: false, var_prefix: Some('$'), ..BASE },
    Spec { lang: Lang::Css, name: "CSS", desc: "Cascade Style Sheets File", exts: &["css", "scss", "less"], line_comment: &[], keywords: CSS_KW, quotes: "\"'", char_lit: false, css: true, ..BASE },
    Spec { lang: Lang::Diff, name: "Diff", desc: "Diff file", exts: &["diff", "patch"], kind: LexKind::Diff, fold: Fold::None, line_comment: &[], block_comment: None, ..BASE },
    Spec { lang: Lang::Go, name: "Go", desc: "Go source file", exts: &["go"], keywords: GO_KW, types: GO_TY, quotes: "\"`", ml_quotes: "`", ..BASE },
    Spec { lang: Lang::Haskell, name: "Haskell", desc: "Haskell", exts: &["hs", "lhs"], fold: Fold::Indent, line_comment: &["--"], block_comment: Some(("{-", "-}")), keywords: HS_KW, char_lit: true, ..BASE },
    Spec { lang: Lang::Html, name: "HTML", desc: "Hyper Text Markup Language file", exts: &["html", "htm", "shtml", "xhtml", "vue", "svelte"], kind: LexKind::Markup, fold: Fold::Tag, line_comment: &[], block_comment: Some(("<!--", "-->")), ..BASE },
    Spec { lang: Lang::Ini, name: "ini file", desc: "MS ini file", exts: &["ini", "inf", "reg", "url", "cfg", "conf", "editorconfig", "gitconfig"], kind: LexKind::Ini, fold: Fold::Section, line_comment: &[";"], block_comment: None, ..BASE },
    Spec { lang: Lang::Java, name: "Java", desc: "Java source file", exts: &["java"], keywords: JAVA_KW, types: JAVA_TY, decorator: true, ..BASE },
    Spec { lang: Lang::JavaScript, name: "JavaScript", desc: "JavaScript file", exts: &["js", "mjs", "cjs", "jsx", "jsm"], keywords: JS_KW, types: JS_TY, quotes: "\"'`", ml_quotes: "`", char_lit: false, var_prefix: None, ..BASE },
    Spec { lang: Lang::Json, name: "JSON", desc: "JSON file", exts: &["json", "jsonc", "json5", "geojson", "webmanifest"], keywords: JSON_KW, char_lit: false, json: true, fold_square: true, ..BASE },
    Spec { lang: Lang::Kotlin, name: "Kotlin", desc: "Kotlin source file", exts: &["kt", "kts"], keywords: KT_KW, types: KT_TY, decorator: true, ..BASE },
    Spec { lang: Lang::Lua, name: "Lua", desc: "Lua source File", exts: &["lua"], fold: Fold::None, line_comment: &["--"], block_comment: Some(("--[[", "]]")), keywords: LUA_KW, builtins: LUA_BI, quotes: "\"'", char_lit: false, ..BASE },
    Spec { lang: Lang::Makefile, name: "Makefile", desc: "Makefile", exts: &["mak", "mk", "makefile", "gnumakefile"], fold: Fold::None, line_comment: &["#"], block_comment: None, keywords: MAKE_KW, quotes: "\"'", char_lit: false, var_prefix: Some('$'), ..BASE },
    Spec { lang: Lang::Markdown, name: "Markdown", desc: "Markdown file", exts: &["md", "markdown", "mdown", "mkd"], kind: LexKind::Markdown, fold: Fold::None, line_comment: &[], block_comment: Some(("<!--", "-->")), ..BASE },
    Spec { lang: Lang::ObjC, name: "Objective-C", desc: "Objective-C source file", exts: &["m", "mm"], keywords: OBJC_KW, types: C_TY, preproc: true, ..BASE },
    Spec { lang: Lang::Pascal, name: "Pascal", desc: "Pascal source file", exts: &["pas", "pp", "dpr", "dpk", "lpr"], fold: Fold::None, line_comment: &["//"], block_comment: Some(("{", "}")), keywords: PAS_KW, nocase: true, quotes: "'", char_lit: false, ..BASE },
    Spec { lang: Lang::Perl, name: "Perl", desc: "Perl source file", exts: &["pl", "pm", "plx", "t"], line_comment: &["#"], block_comment: None, keywords: PERL_KW, quotes: "\"'`", char_lit: false, var_prefix: Some('$'), ..BASE },
    Spec { lang: Lang::Php, name: "PHP", desc: "PHP Hypertext Preprocessor file", exts: &["php", "php3", "php4", "php5", "phps", "phtml"], line_comment: &["//", "#"], keywords: PHP_KW, nocase: true, quotes: "\"'", char_lit: false, var_prefix: Some('$'), ..BASE },
    Spec { lang: Lang::PowerShell, name: "PowerShell", desc: "Windows PowerShell", exts: &["ps1", "psm1", "psd1"], line_comment: &["#"], block_comment: Some(("<#", "#>")), keywords: PS_KW, nocase: true, quotes: "\"'", char_lit: false, var_prefix: Some('$'), ..BASE },
    Spec { lang: Lang::Properties, name: "Properties", desc: "Properties file", exts: &["properties", "env"], kind: LexKind::Ini, fold: Fold::Section, line_comment: &["#", "!"], block_comment: None, ..BASE },
    Spec { lang: Lang::Python, name: "Python", desc: "Python file", exts: &["py", "pyw", "pyi", "pyx", "sconstruct", "sconscript", "wscript"], fold: Fold::Indent, line_comment: &["#"], block_comment: None, keywords: PY_KW, builtins: PY_BI, quotes: "\"'", char_lit: false, triple: true, decorator: true, ..BASE },
    Spec { lang: Lang::R, name: "R", desc: "R programming language", exts: &["r", "rdata", "rds", "rhistory", "rprofile"], line_comment: &["#"], block_comment: None, keywords: R_KW, quotes: "\"'", char_lit: false, ..BASE },
    Spec { lang: Lang::Ruby, name: "Ruby", desc: "Ruby file", exts: &["rb", "rbw", "rake", "gemspec", "gemfile", "rakefile"], fold: Fold::Indent, line_comment: &["#"], block_comment: None, keywords: RUBY_KW, quotes: "\"'`", char_lit: false, var_prefix: Some('@'), ..BASE },
    Spec { lang: Lang::Rust, name: "Rust", desc: "Rust file", exts: &["rs"], keywords: RUST_KW, types: RUST_TY, rust: true, char_lit: true, ml_quotes: "\"", ..BASE },
    Spec { lang: Lang::Shell, name: "Shell", desc: "Unix script file", exts: &["sh", "bsh", "bash", "zsh", "ksh", "csh", "bashrc", "zshrc", "profile"], line_comment: &["#"], block_comment: None, keywords: SH_KW, quotes: "\"'`", ml_quotes: "\"'", char_lit: false, var_prefix: Some('$'), ..BASE },
    Spec { lang: Lang::Sql, name: "SQL", desc: "Structured Query Language file", exts: &["sql", "mysql", "pgsql", "psql"], fold: Fold::None, line_comment: &["--", "#"], keywords: SQL_KW, types: SQL_TY, nocase: true, quotes: "\"'`", char_lit: false, ..BASE },
    Spec { lang: Lang::Swift, name: "Swift", desc: "Swift file", exts: &["swift"], keywords: SWIFT_KW, types: SWIFT_TY, char_lit: false, decorator: true, ..BASE },
    Spec { lang: Lang::Tex, name: "TeX", desc: "TeX file", exts: &["tex", "sty", "cls", "bib", "latex"], fold: Fold::Brace, line_comment: &["%"], block_comment: None, keywords: TEX_KW, quotes: "", char_lit: false, var_prefix: Some('\\'), ..BASE },
    Spec { lang: Lang::Toml, name: "TOML", desc: "Tom's Obvious Minimal Language file", exts: &["toml", "lock"], kind: LexKind::Ini, fold: Fold::Section, line_comment: &["#"], block_comment: None, keywords: TOML_KW, quotes: "\"'", ..BASE },
    Spec { lang: Lang::TypeScript, name: "TypeScript", desc: "TypeScript file", exts: &["ts", "tsx", "mts", "cts"], keywords: TS_KW, types: JS_TY, quotes: "\"'`", ml_quotes: "`", char_lit: false, decorator: true, ..BASE },
    Spec { lang: Lang::VB, name: "Visual Basic", desc: "Visual Basic file", exts: &["vb", "vbs", "bas", "frm", "cls", "vba"], fold: Fold::None, line_comment: &["'"], block_comment: None, keywords: VB_KW, nocase: true, char_lit: false, ..BASE },
    Spec { lang: Lang::Xml, name: "XML", desc: "eXtensible Markup Language file", exts: &["xml", "xsl", "xslt", "xsd", "svg", "xaml", "csproj", "vcxproj", "props", "targets", "resx", "plist", "rss", "atom", "config", "manifest", "nuspec", "wsdl", "xul"], kind: LexKind::Markup, fold: Fold::Tag, line_comment: &[], block_comment: Some(("<!--", "-->")), ..BASE },
    Spec { lang: Lang::Yaml, name: "YAML", desc: "YAML Ain't Markup Language", exts: &["yml", "yaml"], kind: LexKind::Yaml, fold: Fold::Indent, line_comment: &["#"], block_comment: None, keywords: YAML_KW, quotes: "\"'", char_lit: false, ..BASE },
    Spec { lang: Lang::SearchResult, name: "Search results", desc: "Search results", exts: &[], kind: LexKind::SearchResult, fold: Fold::Indent, line_comment: &[], block_comment: None, ..BASE },
];

pub fn spec(lang: Lang) -> &'static Spec {
    SPECS.iter().find(|s| s.lang == lang).unwrap_or(&SPECS[0])
}

pub struct KwSets {
    pub kw: HashSet<String>,
    pub ty: HashSet<String>,
    pub bi: HashSet<String>,
}

pub fn keywords(lang: Lang) -> &'static KwSets {
    static CACHE: OnceLock<Vec<KwSets>> = OnceLock::new();
    let all = CACHE.get_or_init(|| {
        SPECS
            .iter()
            .map(|s| {
                let mk = |src: &str| -> HashSet<String> {
                    src.split_whitespace().map(|w| if s.nocase { w.to_ascii_lowercase() } else { w.to_string() }).collect()
                };
                KwSets { kw: mk(s.keywords), ty: mk(s.types), bi: mk(s.builtins) }
            })
            .collect()
    });
    let idx = SPECS.iter().position(|s| s.lang == lang).unwrap_or(0);
    &all[idx]
}

/// Guess language from a file name.
pub fn detect(path: &std::path::Path) -> Lang {
    let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    match name.as_str() {
        "makefile" | "gnumakefile" => return Lang::Makefile,
        "cmakelists.txt" => return Lang::CMake,
        "dockerfile" => return Lang::Shell,
        "gemfile" | "rakefile" => return Lang::Ruby,
        _ => {}
    }
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if ext.is_empty() {
        return Lang::Text;
    }
    for s in SPECS {
        if s.exts.iter().any(|e| *e == ext) {
            return s.lang;
        }
    }
    Lang::Text
}

/// Guess language from the first line (shebang / xml prolog) when the extension says nothing.
pub fn detect_content(first_line: &str) -> Option<Lang> {
    let l = first_line.trim_start_matches('\u{feff}');
    if l.starts_with("<?xml") {
        return Some(Lang::Xml);
    }
    if l.to_ascii_lowercase().starts_with("<!doctype html") || l.starts_with("<html") {
        return Some(Lang::Html);
    }
    if l.starts_with("#!") {
        if l.contains("python") {
            return Some(Lang::Python);
        }
        if l.contains("sh") {
            return Some(Lang::Shell);
        }
        if l.contains("perl") {
            return Some(Lang::Perl);
        }
        if l.contains("node") {
            return Some(Lang::JavaScript);
        }
        if l.contains("ruby") {
            return Some(Lang::Ruby);
        }
    }
    None
}

/// Token inserted by "Toggle single line comment".
pub fn comment_token(lang: Lang) -> Option<&'static str> {
    let s = spec(lang);
    match lang {
        Lang::Batch => Some("REM"),
        Lang::Html | Lang::Xml | Lang::Markdown | Lang::Css => None,
        _ => s.line_comment.first().copied(),
    }
}
