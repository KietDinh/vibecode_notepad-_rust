//! Function List: regex-based outline of functions / classes / sections per language.

use crate::doc::Document;
use crate::lang::Lang;
use regex::Regex;
use std::sync::OnceLock;

pub struct Entry {
    pub label: String,
    pub line: usize,
}

struct Rules {
    class: Option<Regex>,
    func: Option<Regex>,
    /// C-style "type name(args)" definitions that need `{` on the same or next line.
    c_style: bool,
}

const NOT_FUNCS: &[&str] = &["if", "for", "while", "switch", "return", "sizeof", "catch", "else", "do", "new", "delete", "throw", "case", "using", "typeof", "foreach", "lock", "fixed", "when", "elif"];

fn rules(lang: Lang) -> Option<&'static Rules> {
    static CACHE: OnceLock<Vec<(Lang, Rules)>> = OnceLock::new();
    let all = CACHE.get_or_init(|| {
        let r = |s: &str| Some(Regex::new(s).unwrap());
        let c_func = r(r"^\s*(?:[\w:<>,\*&~\[\]]+\s+)+[\*&]*([A-Za-z_~][\w:~]*)\s*\([^;]*\)\s*(?:const|noexcept|override|final|throws\s+[\w.,\s]+|\s)*\{?\s*(?://.*)?$");
        let class = r(r"^\s*(?:(?:public|private|protected|internal|abstract|sealed|static|final|export|default|partial|data|open)\s+)*(?:class|struct|interface|enum|namespace|record|trait|impl|object)\s+([\w:<>, ]+?)\s*(?:[:{(]|extends|implements|where|$)");
        vec![
            (Lang::C, Rules { class: r(r"^\s*(?:typedef\s+)?struct\s+(\w+)\s*\{?\s*$"), func: c_func.clone(), c_style: true }),
            (Lang::Cpp, Rules { class: class.clone(), func: c_func.clone(), c_style: true }),
            (Lang::ObjC, Rules { class: r(r"^\s*@(?:interface|implementation)\s+(\w+)"), func: r(r"^\s*[-+]\s*\([^)]*\)\s*(\w+)"), c_style: false }),
            (Lang::CSharp, Rules { class: class.clone(), func: c_func.clone(), c_style: true }),
            (Lang::Java, Rules { class: class.clone(), func: c_func.clone(), c_style: true }),
            (Lang::Kotlin, Rules { class: class.clone(), func: r(r"^\s*(?:[\w]+\s+)*fun\s+(?:<[^>]*>\s*)?(?:[\w.]+\.)?(\w+)\s*\("), c_style: false }),
            (Lang::Swift, Rules { class: class.clone(), func: r(r"^\s*(?:[\w@]+\s+)*func\s+(\w+)"), c_style: false }),
            (Lang::Rust, Rules { class: r(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|trait|impl(?:<[^>]*>)?|mod)\s+([\w:<>, ]+?)\s*(?:\{|;|where|for|$)"), func: r(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:default\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+\S+\s+)?fn\s+(\w+)"), c_style: false }),
            (Lang::Go, Rules { class: r(r"^type\s+(\w+)\s+(?:struct|interface)"), func: r(r"^func\s+(?:\([^)]*\)\s*)?(\w+)"), c_style: false }),
            (Lang::Python, Rules { class: r(r"^\s*class\s+(\w+)"), func: r(r"^\s*(?:async\s+)?def\s+(\w+)"), c_style: false }),
            (Lang::JavaScript, Rules { class: r(r"^\s*(?:export\s+)?(?:default\s+)?class\s+(\w+)"), func: r(r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?(?:function\s*\*?\s*(\w+)|(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s*)?(?:function\b|\([^)]*\)\s*=>|\w+\s*=>)|(?:static\s+)?(?:async\s+)?(\w+)\s*\([^)]*\)\s*\{\s*$)"), c_style: false }),
            (Lang::TypeScript, Rules { class: r(r"^\s*(?:export\s+)?(?:default\s+)?(?:abstract\s+)?(?:class|interface|enum)\s+(\w+)"), func: r(r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?(?:function\s*\*?\s*(\w+)|(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s*)?(?:function\b|\([^)]*\)\s*(?::\s*[^=]+)?=>)|(?:public\s+|private\s+|protected\s+|static\s+|readonly\s+)*(?:async\s+)?(\w+)\s*(?:<[^>]*>)?\([^)]*\)\s*(?::\s*[^{]+)?\{\s*$)"), c_style: false }),
            (Lang::Php, Rules { class: r(r"^\s*(?:abstract\s+|final\s+)?(?:class|interface|trait)\s+(\w+)"), func: r(r"^\s*(?:(?:public|private|protected|static|abstract|final)\s+)*function\s+&?(\w+)"), c_style: false }),
            (Lang::Ruby, Rules { class: r(r"^\s*(?:class|module)\s+([\w:]+)"), func: r(r"^\s*def\s+([\w.?!=]+)"), c_style: false }),
            (Lang::Lua, Rules { class: None, func: r(r"^\s*(?:local\s+)?function\s+([\w.:]+)|^\s*(?:local\s+)?([\w.]+)\s*=\s*function\b"), c_style: false }),
            (Lang::Perl, Rules { class: r(r"^\s*package\s+([\w:]+)"), func: r(r"^\s*sub\s+(\w+)"), c_style: false }),
            (Lang::Shell, Rules { class: None, func: r(r"^\s*(?:function\s+)?([\w-]+)\s*\(\)\s*\{?|^\s*function\s+([\w-]+)"), c_style: false }),
            (Lang::PowerShell, Rules { class: r(r"^\s*class\s+(\w+)"), func: r(r"(?i)^\s*(?:function|filter)\s+([\w-]+)"), c_style: false }),
            (Lang::Batch, Rules { class: None, func: r(r"^:(\w[\w.-]*)"), c_style: false }),
            (Lang::Ini, Rules { class: None, func: r(r"^\s*\[([^\]]+)\]"), c_style: false }),
            (Lang::Properties, Rules { class: None, func: r(r"^\s*\[([^\]]+)\]"), c_style: false }),
            (Lang::Toml, Rules { class: None, func: r(r"^\s*\[\[?([^\]]+)\]\]?"), c_style: false }),
            (Lang::Markdown, Rules { class: None, func: r(r"^(#{1,6}\s+.+)$"), c_style: false }),
            (Lang::Sql, Rules { class: r(r"(?i)^\s*create\s+(?:or\s+replace\s+)?table\s+([\w.\[\]`]+)"), func: r(r"(?i)^\s*create\s+(?:or\s+(?:replace|alter)\s+)?(?:procedure|function|view|trigger)\s+([\w.\[\]`]+)"), c_style: false }),
            (Lang::Makefile, Rules { class: None, func: r(r"^([\w.%/-]+)\s*:(?:[^=]|$)"), c_style: false }),
            (Lang::VB, Rules { class: r(r"(?i)^\s*(?:public\s+|private\s+|friend\s+)?(?:class|module|structure)\s+(\w+)"), func: r(r"(?i)^\s*(?:(?:public|private|friend|protected|shared|overrides|overridable)\s+)*(?:sub|function|property)\s+(?:get\s+|let\s+|set\s+)?(\w+)"), c_style: false }),
            (Lang::Pascal, Rules { class: None, func: r(r"(?i)^\s*(?:procedure|function|constructor|destructor)\s+([\w.]+)"), c_style: false }),
        ]
    });
    all.iter().find(|(l, _)| *l == lang).map(|(_, r)| r)
}

fn first_group(c: &regex::Captures) -> Option<String> {
    (1..c.len()).find_map(|i| c.get(i)).map(|m| m.as_str().trim().to_string())
}

pub fn parse(doc: &Document) -> Option<Vec<Entry>> {
    let rules = rules(doc.lang)?;
    let mut out = Vec::new();
    let n = doc.line_count().min(60_000);
    let mut in_block = false;
    for l in 0..n {
        let text = doc.line_text(l);
        let t = text.trim_start();
        // skip comment lines and C block comments
        if in_block {
            if t.contains("*/") {
                in_block = false;
            }
            continue;
        }
        if t.starts_with("/*") && !t.contains("*/") {
            in_block = true;
            continue;
        }
        if t.starts_with("//") || (t.starts_with('#') && doc.lang != Lang::Markdown && doc.lang != Lang::C && doc.lang != Lang::Cpp) || t.starts_with('*') {
            continue;
        }
        if t.len() > 400 {
            continue;
        }
        let indent = text.len() - t.len();
        let pad = if indent > 0 { "    " } else { "" };
        if let Some(re) = &rules.class {
            if let Some(c) = re.captures(&text) {
                if let Some(name) = first_group(&c) {
                    if !name.is_empty() {
                        out.push(Entry { label: format!("{pad}\u{25C6} {name}"), line: l });
                        continue;
                    }
                }
            }
        }
        if let Some(re) = &rules.func {
            if let Some(c) = re.captures(&text) {
                let Some(name) = first_group(&c) else { continue };
                let base = name.rsplit("::").next().unwrap_or(&name);
                if (NOT_FUNCS.contains(&base) && (rules.c_style || base != "new")) || name.is_empty() {
                    continue;
                }
                if rules.c_style {
                    let has_brace = text.trim_end().ends_with('{') || (l + 1 < doc.line_count() && doc.line_text(l + 1).trim_start().starts_with('{'));
                    if !has_brace || text.trim_end().ends_with(';') {
                        continue;
                    }
                }
                let label = if doc.lang == Lang::Markdown {
                    let level = name.chars().take_while(|c| *c == '#').count();
                    format!("{}{}", "  ".repeat(level.saturating_sub(1)), name.trim_start_matches('#').trim())
                } else if matches!(doc.lang, Lang::Ini | Lang::Properties | Lang::Toml | Lang::Batch | Lang::Makefile) {
                    name
                } else {
                    format!("{pad}{name}()")
                };
                out.push(Entry { label, line: l });
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(lang: Lang, text: &str) -> Document {
        let mut d = Document::new("t".into());
        d.set_content(text);
        d.lang = lang;
        d
    }

    #[test]
    fn finds_functions() {
        let d = doc(Lang::Cpp, "#include <x>\nint main(int argc, char** argv) {\n  if (x) {\n  }\n}\nstatic void helper()\n{\n}\nint decl(int);\n");
        let names: Vec<String> = parse(&d).unwrap().into_iter().map(|e| e.label).collect();
        assert_eq!(names, vec!["main()", "helper()"]);
        let d = doc(Lang::Python, "class A:\n    def f(self):\n        pass\ndef g():\n    pass\n");
        let names: Vec<String> = parse(&d).unwrap().into_iter().map(|e| e.label).collect();
        assert_eq!(names, vec!["\u{25C6} A", "    f()", "g()"]);
        let d = doc(Lang::Rust, "pub struct S;\nimpl S {\n    pub fn new() -> Self { S }\n}\nfn main() {}\n");
        let names: Vec<String> = parse(&d).unwrap().into_iter().map(|e| e.label).collect();
        assert_eq!(names, vec!["\u{25C6} S", "\u{25C6} S", "    new()", "main()"]);
    }
}
