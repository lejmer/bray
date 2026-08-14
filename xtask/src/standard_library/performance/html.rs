use std::fmt::Write as _;
use std::fs;
use std::path::Path;

pub(super) const MAX_HTML_BYTES: usize = 8 * 1024 * 1024;

pub(super) struct BoundedHtml {
    contents: String,
    overflowed: bool,
}

impl BoundedHtml {
    pub(super) fn new() -> Self {
        Self {
            contents: String::new(),
            overflowed: false,
        }
    }

    pub(super) fn push_str(&mut self, value: &str) {
        if self.contents.len().saturating_add(value.len()) > MAX_HTML_BYTES {
            self.overflowed = true;
            return;
        }

        self.contents.push_str(value);
    }
}

impl std::fmt::Write for BoundedHtml {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        self.push_str(value);

        if self.overflowed {
            Err(std::fmt::Error)
        } else {
            Ok(())
        }
    }
}

pub(super) fn document_start(title: &str) -> BoundedHtml {
    let mut html = BoundedHtml::new();

    let _ = write!(
        html,
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
        <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
        <title>{}</title><style>{}</style></head><body><main><h1>{}</h1>",
        escape(title),
        CSS,
        escape(title),
    );

    html
}

pub(super) fn finish(mut html: BoundedHtml) -> Result<String, String> {
    html.push_str("</main></body></html>");

    if html.overflowed {
        return Err(format!(
            "HTML performance report exceeds its {} byte bound",
            MAX_HTML_BYTES
        ));
    }

    Ok(html.contents)
}

pub(super) fn write(path: &Path, html: String) -> Result<(), String> {
    fs::write(path, html).map_err(|error| format!("could not write {}: {error}", path.display()))
}

pub(super) fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());

    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }

    escaped
}

const CSS: &str = r#"
:root { color-scheme: light dark; font-family: system-ui, sans-serif; line-height: 1.45; }
body { margin: 0; background: Canvas; color: CanvasText; }
main { max-width: 1200px; margin: 0 auto; padding: 2rem; }
section { margin: 2rem 0; }
table { border-collapse: collapse; width: 100%; font-variant-numeric: tabular-nums; }
th, td { border-bottom: 1px solid color-mix(in srgb, CanvasText 20%, transparent); padding: .6rem; text-align: right; white-space: nowrap; }
th:first-child, td:first-child { text-align: left; }
.table-scroll { overflow-x: auto; }
.bray-row > * { font-weight: 700; }
.metric-best { background: color-mix(in srgb, #3da35d 18%, Canvas); box-shadow: inset 0 0 0 1px color-mix(in srgb, #3da35d 45%, Canvas); }
dl { display: grid; grid-template-columns: max-content 1fr; gap: .35rem 1rem; }
dt { font-weight: 650; }
dd { margin: 0; overflow-wrap: anywhere; }
code { font-family: ui-monospace, monospace; }
.improved { color: #16803c; }
.regressed { color: #c43b32; }
.indeterminate { color: #767676; }
@media (prefers-color-scheme: dark) { .improved { color: #65d58a; } .regressed { color: #ff8178; } .indeterminate { color: #aaa; } }
"#;
