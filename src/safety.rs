//! Security gates applied to untrusted Markdown: what may reach an external
//! process, the filesystem, or the XML of the generated document.

use std::borrow::Cow;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::config::ImageAccess;
use crate::error::{CrateError, Result};

// ------------------------------------------------------------- XML text

/// `Char` production of XML 1.0. Anything else makes Word report the file as
/// corrupt, so it must never reach `document.xml`.
pub(crate) fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
}

/// Remove characters that cannot appear in XML 1.0 (control characters,
/// U+FFFE/U+FFFF). Borrowed when the input is already clean.
pub(crate) fn sanitize_xml_chars(s: &str) -> Cow<'_, str> {
    if s.chars().all(is_xml_char) {
        Cow::Borrowed(s)
    } else {
        Cow::Owned(s.chars().filter(|&c| is_xml_char(c)).collect())
    }
}

// ----------------------------------------------------------------- TeX

/// Control sequences that read or write files, run programs, load code, or
/// change how later text is tokenised (which could rebuild a blocked name).
const TEX_DENIED: &[&str] = &[
    "input",
    "include",
    "includeonly",
    "InputIfFileExists",
    "IfFileExists",
    "openin",
    "openout",
    "read",
    "readline",
    "write",
    "immediate",
    "newread",
    "newwrite",
    "closein",
    "closeout",
    "usepackage",
    "RequirePackage",
    "documentclass",
    "LoadClass",
    "PassOptionsToPackage",
    "verbatiminput",
    "lstinputlisting",
    "includegraphics",
    "includepdf",
    "catcode",
    "csname",
    "endcsname",
    "scantokens",
    "everyeof",
    "special",
    "font",
    "makeatletter",
    "makeatother",
    "ShellEscape",
    "write18",
    "directlua",
    "latelua",
    "luaescapestring",
];

/// Reject LaTeX that tries to touch the host before it is handed to `latex`.
///
/// This is one layer; the other is running TeX in paranoid file-access mode
/// (see `renderer`). The check works on control-sequence names, and also
/// refuses `^^` (TeX's character-code notation, e.g. `\^^69nput` is `\input`)
/// and the env-name `filecontents`, so the usual spellings cannot sneak past.
pub(crate) fn check_tex(latex: &str) -> std::result::Result<(), String> {
    if latex.contains("^^") {
        return Err("`^^` character-code notation is not allowed".into());
    }
    if latex.contains("filecontents") {
        return Err("`filecontents` is not allowed".into());
    }
    let mut chars = latex.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            continue;
        }
        let mut name = String::new();
        while let Some(&n) = chars.peek() {
            if n.is_ascii_alphabetic() || n == '@' {
                name.push(n);
                chars.next();
            } else {
                break;
            }
        }
        if name.is_empty() {
            chars.next(); // `\\`, `\{`, `\,` ... : a single-character control symbol
            continue;
        }
        if TEX_DENIED.contains(&name.as_str())
            || name.starts_with("pdf")
            || name.starts_with("lua")
            || name.contains('@')
        {
            return Err(format!("the TeX command `\\{name}` is not allowed"));
        }
    }
    Ok(())
}

// --------------------------------------------------------------- links

/// Return the URL to link to, or `None` when the link must be shown as text.
///
/// Only absolute URLs whose scheme is in `allowed` pass. Everything else is
/// refused: `javascript:`, `file:`, `data:`, app-handler schemes such as
/// `ms-excel:`, Windows drive and UNC paths (`\\host\share` leaks credentials
/// when opened), protocol-relative `//host`, and relative paths or `#anchors`
/// that Word would resolve against the local disk.
pub(crate) fn safe_link(url: &str, allowed: &[String]) -> Option<String> {
    // Browsers and Word ignore ASCII whitespace/control characters inside the
    // scheme (`java\tscript:`), so judge the URL with them removed.
    let url: String = url.chars().filter(|c| !c.is_control() && !c.is_whitespace()).collect();
    // Characters that cannot appear in XML (U+FFFE and U+FFFF, which a numeric reference in the
    // destination can produce) would make the relationships part unreadable.
    if url.is_empty() || url.len() > 2048 || !url.chars().all(is_xml_char) {
        return None;
    }
    let colon = url.find(':')?;
    let scheme = &url[..colon];
    let valid = scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    // A drive letter ("C:\\x") parses as a one-letter scheme and is not allowed.
    if valid && allowed.iter().any(|a| a.eq_ignore_ascii_case(scheme)) {
        if scheme.eq_ignore_ascii_case("mailto") {
            return clean_mailto(&url);
        }
        Some(url)
    } else {
        None
    }
}

/// A `mailto:` link keeps its address and the header fields a person would expect a link to
/// prefill (`subject`, `body`, `cc`, `bcc`). Anything else in the query (`attach=`, `from=`,
/// private extension headers that a few mail clients act on) is dropped.
///
/// There is no fragment (a mail client may read `#` text as more of the query), and an address
/// that hides `?` or `&` behind a percent escape is refused (`None`: the text stays, unlinked).
fn clean_mailto(url: &str) -> Option<String> {
    let url = url.split_once('#').map_or(url, |(u, _)| u);
    let (head, query) = match url.split_once('?') {
        Some((h, q)) => (h, q),
        None => (url, ""),
    };
    let lower = head.to_ascii_lowercase();
    if lower.contains("%3f") || lower.contains("%26") {
        return None;
    }
    let kept: Vec<&str> = query
        .split('&')
        .filter(|pair| {
            let key = pair.split('=').next().unwrap_or("");
            let key = percent_encoding::percent_decode_str(key)
                .decode_utf8_lossy()
                .to_ascii_lowercase();
            matches!(key.as_str(), "subject" | "body" | "cc" | "bcc")
        })
        .collect();
    let mut out = head.to_string();
    if !kept.is_empty() {
        out.push('?');
        out.push_str(&kept.join("&"));
    }
    Some(out)
}

// -------------------------------------------------------------- images

/// Read a local image named by a Markdown reference, subject to `access`.
pub(crate) fn read_local_image(
    reference: &str,
    base_dir: &Path,
    access: ImageAccess,
    max_bytes: u64,
) -> Result<(PathBuf, Vec<u8>)> {
    let raw = reference.strip_prefix("file://").unwrap_or(reference);
    let decoded = percent_encoding::percent_decode_str(raw)
        .decode_utf8_lossy()
        .into_owned();
    if decoded.contains('\0') {
        return Err(CrateError::UnsafeContent("image path contains a NUL byte".into()));
    }
    let mut path = PathBuf::from(&decoded);
    if path.is_relative() {
        path = base_dir.join(path);
    }

    match access {
        ImageAccess::None => {
            return Err(CrateError::UnsafeContent("local images are disabled".into()));
        }
        ImageAccess::BaseDir => {
            let outside = || {
                CrateError::UnsafeContent(format!(
                    "image `{reference}` is outside the base directory {}",
                    base_dir.display()
                ))
            };
            // Network-share and device paths are refused before anything touches the
            // filesystem: on Windows, merely resolving `\\host\share\x` contacts the host.
            if decoded.starts_with("\\\\") || decoded.starts_with("//") {
                return Err(outside());
            }
            let base = fs::canonicalize(base_dir).map_err(|e| CrateError::io(base_dir, e))?;
            // Decide from the text alone whether the path can stay inside the base, so that
            // paths pointing elsewhere get the same answer whether or not they exist.
            let candidate = if Path::new(&decoded).is_relative() {
                base.join(&decoded)
            } else {
                PathBuf::from(&decoded)
            };
            // On Windows a canonical path is `\\?\C:\...` while an absolute reference is `C:\...`;
            // accept either spelling of the base here, and let the real check below decide.
            let given_base = std::path::absolute(base_dir)
                .ok()
                .and_then(|p| lexically_normalized(&p));
            match lexically_normalized(&candidate) {
                Some(norm) if norm.starts_with(&base) || given_base.as_ref().is_some_and(|b| norm.starts_with(b)) => {
                    path = norm;
                }
                _ => return Err(outside()),
            }
            path = fs::canonicalize(&path).map_err(|e| CrateError::io(&path, e))?;
            if !path.starts_with(&base) {
                return Err(outside());
            }
        }
        ImageAccess::Any => {}
    }

    let bytes = read_regular_file(&path, reference, max_bytes)?;
    Ok((path, bytes))
}

/// `path` with `.` and `..` resolved by text alone; `None` when a `..` would climb above
/// the root.
fn lexically_normalized(path: &Path) -> Option<PathBuf> {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    Some(out)
}

/// Read a regular file of at most `max_bytes`, never more than that into memory.
///
/// The file is inspected before it is opened: opening a FIFO blocks forever,
/// and devices such as `/dev/zero` never end. `label` is how the caller names
/// the file in error messages.
pub(crate) fn read_regular_file(path: &Path, label: &str, max_bytes: u64) -> Result<Vec<u8>> {
    let meta = fs::metadata(path).map_err(|e| CrateError::io(path, e))?;
    if !meta.is_file() {
        return Err(CrateError::UnsafeContent(format!("`{label}` is not a regular file")));
    }
    if meta.len() > max_bytes {
        return Err(CrateError::LimitExceeded(format!(
            "file {} is {} bytes (limit {max_bytes})",
            path.display(),
            meta.len()
        )));
    }
    let mut buf = Vec::with_capacity(usize::try_from(meta.len()).unwrap_or(0));
    fs::File::open(path)
        .and_then(|f| f.take(max_bytes + 1).read_to_end(&mut buf))
        .map_err(|e| CrateError::io(path, e))?;
    if buf.len() as u64 > max_bytes {
        // The file grew while we read it.
        return Err(CrateError::LimitExceeded(format!(
            "file {} exceeds {max_bytes} bytes",
            path.display()
        )));
    }
    Ok(buf)
}

/// Wraps a seekable writer and refuses to let the archive grow past `max` bytes
/// (counted from where the writer started), so an over-large result fails
/// early instead of filling a disk or a response buffer.
pub(crate) struct LimitedWriter<W> {
    inner: W,
    start: u64,
    pos: u64,
    max: u64,
    /// Set when a write was refused for size, to tell that apart from an I/O error.
    pub(crate) exceeded: bool,
}

impl<W: std::io::Write + std::io::Seek> LimitedWriter<W> {
    pub(crate) fn new(mut inner: W, max: u64) -> std::io::Result<Self> {
        let start = inner.stream_position()?;
        Ok(LimitedWriter {
            inner,
            start,
            pos: start,
            max,
            exceeded: false,
        })
    }
}

impl<W> LimitedWriter<W> {
    /// The wrapped writer.
    pub(crate) fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: std::io::Write + std::io::Seek> std::io::Write for LimitedWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.exceeded {
            // The failure was already reported. Swallow what the archive writer's
            // clean-up still tries to write, so it does not print a second error.
            return Ok(buf.len());
        }
        let end = self.pos.saturating_add(buf.len() as u64);
        if end - self.start > self.max {
            self.exceeded = true;
            return Err(std::io::Error::other("output size limit exceeded"));
        }
        let n = self.inner.write(buf)?;
        self.pos += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl<W: std::io::Write + std::io::Seek> std::io::Seek for LimitedWriter<W> {
    fn seek(&mut self, from: std::io::SeekFrom) -> std::io::Result<u64> {
        self.pos = self.inner.seek(from)?;
        Ok(self.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mailto_keeps_only_the_prefill_fields() {
        let allowed = vec!["mailto".to_string()];
        let c = |u: &str| safe_link(u, &allowed).unwrap();
        assert_eq!(c("mailto:a@b.example"), "mailto:a@b.example");
        assert_eq!(
            c("mailto:a@b.example?subject=Hi&body=Text"),
            "mailto:a@b.example?subject=Hi&body=Text"
        );
        assert_eq!(
            c("mailto:a@b.example?attach=/etc/passwd&subject=Hi"),
            "mailto:a@b.example?subject=Hi"
        );
        assert_eq!(
            c("mailto:a@b.example?%61ttach=/x&cc=c@d.example"),
            "mailto:a@b.example?cc=c@d.example"
        );
        assert_eq!(c("mailto:a@b.example?attach=/etc/passwd"), "mailto:a@b.example");
        assert_eq!(c("MAILTO:a@b.example?Attachment=x&BODY=y"), "MAILTO:a@b.example?BODY=y");
    }

    #[test]
    fn xml_sanitising_drops_only_illegal_characters() {
        assert!(matches!(
            sanitize_xml_chars("plain\ttext\r\n é 日本 😀"),
            Cow::Borrowed(_)
        ));
        assert_eq!(sanitize_xml_chars("a\u{1}b\u{b}c\u{1f}d\u{FFFE}e\u{FFFF}f"), "abcdef");
        assert_eq!(sanitize_xml_chars("keep\tthis\nand\rthis"), "keep\tthis\nand\rthis");
    }

    #[test]
    fn ordinary_math_passes_the_tex_check() {
        for ok in [
            r"\frac{a}{b} + \sqrt{x^2}",
            r"\sum_{i=1}^{n} \alpha_i \leq \infty",
            r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}",
            r"\text{inputs and reads} \mathbf{x}' \hat{y} \^{a} \\ \{ \}",
            r"\inputlineno \readsomething", // different tokens, not \input / \read
            r"\newcommand{\R}{\mathbb{R}} \R^n",
        ] {
            assert_eq!(check_tex(ok), Ok(()), "{ok}");
        }
    }

    #[test]
    fn file_and_shell_primitives_are_refused() {
        for bad in [
            r"\input{/etc/passwd}",
            r"x \input /etc/passwd",
            r"\include{secrets}",
            r"\openin5=/etc/passwd \read5 to \x",
            r"\immediate\write18{rm -rf /}",
            r"\openout3=evil.tex",
            r"\usepackage{shellesc}",
            r"\documentclass{article}",
            r"\catcode`\^^M=12",
            r"\expandafter\csname in\string put\endcsname",
            r"\scantokens{\input x}",
            r"\^^69nput{/etc/passwd}",
            r"\special{psfile=/etc/passwd}",
            r"\pdfshellescape",
            r"\pdffiledump offset 0 length 10{/etc/passwd}",
            r"\directlua{os.execute('id')}",
            r"\makeatletter \@@input x",
            r"\begin{filecontents}{x.tex}hi\end{filecontents}",
            r"\font\x=/etc/passwd",
            r"\verbatiminput{/etc/passwd}",
            r"\includegraphics{/etc/passwd}",
        ] {
            assert!(check_tex(bad).is_err(), "should be refused: {bad}");
        }
    }

    fn allow() -> Vec<String> {
        ["http", "https", "mailto"].map(String::from).to_vec()
    }

    #[test]
    fn only_allowlisted_schemes_become_links() {
        let a = allow();
        assert_eq!(
            safe_link("https://example.com/a?b=1&c=2", &a).as_deref(),
            Some("https://example.com/a?b=1&c=2")
        );
        assert_eq!(
            safe_link("HTTP://EXAMPLE.com", &a).as_deref(),
            Some("HTTP://EXAMPLE.com")
        );
        assert_eq!(
            safe_link("mailto:me@example.com", &a).as_deref(),
            Some("mailto:me@example.com")
        );
        for bad in [
            "javascript:alert(1)",
            " JaVaScRiPt:alert(1)",
            "java\tscript:alert(1)",
            "java\nscript:alert(1)",
            "file:///etc/passwd",
            r"\\evil\share\x",
            "//evil.example/x",
            r"C:\Windows\System32\calc.exe",
            "ms-excel:ofe|u|http://x",
            "search-ms:query=x",
            "data:text/html;base64,AAAA",
            "vbscript:x",
            "#anchor",
            "relative/path.md",
            "../up.md",
            "",
            "   ",
            ":nothing",
            "1http://x",
        ] {
            assert_eq!(safe_link(bad, &a), None, "{bad:?}");
        }
        assert_eq!(
            safe_link(&format!("https://x/{}", "a".repeat(3000)), &a),
            None,
            "absurdly long URL"
        );
        // The allowlist is configurable.
        assert!(safe_link("ftp://example.com", &["ftp".to_string()]).is_some());
    }

    #[test]
    fn odd_percent_escapes_never_panic() {
        let dir = tempfile::tempdir().unwrap();
        for weird in [
            "%é%zz%4",
            "100%",
            "x%FFy",
            "日本%E6語%",
            "%00",
            "%",
            "%%%",
            "caf%C3%A9.png",
            "\u{202e}",
        ] {
            for access in [ImageAccess::BaseDir, ImageAccess::Any, ImageAccess::None] {
                let _ = read_local_image(weird, dir.path(), access, 100); // an error is fine; a panic is not
            }
        }
        fs::write(dir.path().join("café.png"), b"ok").unwrap();
        assert!(read_local_image("caf%C3%A9.png", dir.path(), ImageAccess::BaseDir, 10).is_ok());
    }

    #[test]
    fn image_access_policy() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("docs");
        fs::create_dir_all(base.join("sub")).unwrap();
        fs::write(base.join("a.png"), b"12345").unwrap();
        fs::write(base.join("sub/b.png"), b"12345").unwrap();
        fs::write(dir.path().join("secret.png"), b"secret").unwrap();

        let read = |r: &str, access, max| read_local_image(r, &base, access, max);
        assert!(read("a.png", ImageAccess::BaseDir, 100).is_ok());
        assert!(read("sub/b.png", ImageAccess::BaseDir, 100).is_ok());
        assert!(read("sub/../a.png", ImageAccess::BaseDir, 100).is_ok());
        assert!(
            read("a%2Epng", ImageAccess::BaseDir, 100).is_ok(),
            "percent-escapes are decoded first"
        );
        assert!(matches!(
            read("../secret.png", ImageAccess::BaseDir, 100),
            Err(CrateError::UnsafeContent(_))
        ));
        assert!(matches!(
            read("sub/../../secret.png", ImageAccess::BaseDir, 100),
            Err(CrateError::UnsafeContent(_))
        ));
        let abs = dir.path().join("secret.png");
        assert!(matches!(
            read(abs.to_str().unwrap(), ImageAccess::BaseDir, 100),
            Err(CrateError::UnsafeContent(_))
        ));
        assert!(read(abs.to_str().unwrap(), ImageAccess::Any, 100).is_ok());
        assert!(matches!(
            read("a.png", ImageAccess::None, 100),
            Err(CrateError::UnsafeContent(_))
        ));
        assert!(matches!(
            read("a.png", ImageAccess::BaseDir, 4),
            Err(CrateError::LimitExceeded(_))
        ));
        assert!(matches!(
            read("a\0.png", ImageAccess::Any, 100),
            Err(CrateError::UnsafeContent(_))
        ));
        assert!(matches!(
            read("missing.png", ImageAccess::BaseDir, 100),
            Err(CrateError::Io { .. })
        ));
        assert!(
            matches!(
                read("sub", ImageAccess::BaseDir, 100),
                Err(CrateError::UnsafeContent(_))
            ),
            "directory"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_and_special_files_cannot_escape() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("docs");
        fs::create_dir_all(&base).unwrap();
        fs::write(dir.path().join("secret.png"), b"secret").unwrap();
        symlink(dir.path().join("secret.png"), base.join("link.png")).unwrap();
        symlink("/etc", base.join("etc")).unwrap();
        assert!(matches!(
            read_local_image("link.png", &base, ImageAccess::BaseDir, 100),
            Err(CrateError::UnsafeContent(_))
        ));
        assert!(matches!(
            read_local_image("etc/passwd", &base, ImageAccess::BaseDir, 100),
            Err(CrateError::UnsafeContent(_))
        ));
        // Endless / blocking files are rejected without being read, even with full access.
        for special in ["/dev/zero", "/dev/null", "/dev/urandom"] {
            assert!(
                matches!(
                    read_local_image(special, &base, ImageAccess::Any, 1 << 20),
                    Err(CrateError::UnsafeContent(_))
                ),
                "{special}"
            );
        }
    }
}
