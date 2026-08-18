//! Package plain text into a real Office file when the user asked for Word/Excel.
//!
//! `write` is UTF-8. A `.docx` that is only text is not a Word document — that
//! is why earlier sessions fell back to RTF-as-`.doc`. Packaging here is the
//! engine default: no Python, no Word/WPS guess.

use std::path::{Path, PathBuf};

/// If `path` is an Office deliverable, return `(final_path, zip_bytes)`.
/// Legacy `.doc` / `.xls` are upgraded to `.docx` / `.xlsx`.
pub fn maybe_package(path: &Path, text: &str) -> Option<(PathBuf, Vec<u8>)> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "docx" => Some((path.to_path_buf(), package_docx(text))),
        "doc" => Some((path.with_extension("docx"), package_docx(text))),
        "xlsx" => Some((path.to_path_buf(), package_xlsx(text))),
        "xls" => Some((path.with_extension("xlsx"), package_xlsx(text))),
        _ => None,
    }
}

pub fn package_docx(text: &str) -> Vec<u8> {
    let body = docx_body_xml(text);
    zip_store(&[
        (
            "[Content_Types].xml",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#,
        ),
        (
            "_rels/.rels",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#,
        ),
        (
            "word/_rels/document.xml.rels",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>
"#,
        ),
        ("word/document.xml", body.as_bytes()),
    ])
}

pub fn package_xlsx(text: &str) -> Vec<u8> {
    let sheet = xlsx_sheet_xml(text);
    zip_store(&[
        (
            "[Content_Types].xml",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
  <Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>"#,
        ),
        (
            "_rels/.rels",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#,
        ),
        (
            "xl/_rels/workbook.xml.rels",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
</Relationships>"#,
        ),
        (
            "xl/workbook.xml",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets>
</workbook>"#,
        ),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
    ])
}

fn docx_body_xml(text: &str) -> String {
    let blocks = parse_doc_blocks(text);
    let mut body = String::new();
    if blocks.is_empty() {
        body.push_str("<w:p/>");
    } else {
        for block in blocks {
            match block {
                DocBlock::Heading { level, text } => {
                    body.push_str(&heading_xml(level, &text));
                }
                DocBlock::Paragraph(text) => body.push_str(&para_xml(&text)),
                DocBlock::Table(rows) => body.push_str(&table_xml(&rows)),
                DocBlock::Empty => body.push_str("<w:p/>"),
            }
        }
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>{body}<w:sectPr/></w:body>
</w:document>"#
    )
}

enum DocBlock {
    Heading { level: u8, text: String },
    Paragraph(String),
    Table(Vec<Vec<String>>),
    Empty,
}

fn parse_doc_blocks(text: &str) -> Vec<DocBlock> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    if lines.iter().all(|l| l.is_empty()) {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.is_empty() {
            if !matches!(out.last(), Some(DocBlock::Empty)) {
                out.push(DocBlock::Empty);
            }
            i += 1;
            continue;
        }
        if let Some((level, title)) = parse_md_heading(trimmed) {
            out.push(DocBlock::Heading {
                level,
                text: strip_md_inline(title),
            });
            i += 1;
            continue;
        }
        if is_md_table_row(trimmed) {
            let mut rows = Vec::new();
            while i < lines.len() && is_md_table_row(lines[i].trim()) {
                let row = lines[i].trim();
                if !is_md_table_sep(row) {
                    rows.push(split_md_row(row));
                }
                i += 1;
            }
            if !rows.is_empty() {
                out.push(DocBlock::Table(rows));
            }
            continue;
        }
        out.push(DocBlock::Paragraph(strip_md_inline(trimmed)));
        i += 1;
    }
    while matches!(out.last(), Some(DocBlock::Empty)) {
        out.pop();
    }
    out
}

fn parse_md_heading(line: &str) -> Option<(u8, &str)> {
    let bytes = line.as_bytes();
    let mut n = 0usize;
    while n < bytes.len() && bytes[n] == b'#' && n < 6 {
        n += 1;
    }
    if n == 0 || n >= bytes.len() || bytes[n] != b' ' {
        return None;
    }
    let title = line[n + 1..].trim();
    if title.is_empty() {
        return None;
    }
    Some((n as u8, title))
}

fn is_md_table_row(line: &str) -> bool {
    line.starts_with('|') && line.chars().filter(|c| *c == '|').count() >= 2
}

fn is_md_table_sep(line: &str) -> bool {
    let inner = line.trim().trim_matches('|');
    !inner.is_empty()
        && inner
            .chars()
            .all(|c| c == '-' || c == ':' || c == '|' || c == ' ')
}

fn split_md_row(line: &str) -> Vec<String> {
    let s = line.trim();
    let s = s.strip_prefix('|').unwrap_or(s);
    let s = s.strip_suffix('|').unwrap_or(s);
    s.split('|').map(|c| strip_md_inline(c.trim())).collect()
}

fn strip_md_inline(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' | '_' => {
                let mark = chars[i];
                while i < chars.len() && chars[i] == mark {
                    i += 1;
                }
            }
            '`' => i += 1,
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

fn heading_xml(level: u8, text: &str) -> String {
    let half_pts = match level {
        1 => 32,
        2 => 28,
        _ => 24,
    };
    format!(
        "<w:p><w:pPr><w:spacing w:before=\"240\" w:after=\"80\"/></w:pPr>\
         <w:r><w:rPr><w:b/><w:sz w:val=\"{half_pts}\"/><w:szCs w:val=\"{half_pts}\"/></w:rPr>\
         <w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        xml_escape(text)
    )
}

fn para_xml(text: &str) -> String {
    format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        xml_escape(text)
    )
}

fn table_xml(rows: &[Vec<String>]) -> String {
    let mut body = String::from(
        "<w:tbl><w:tblPr><w:tblW w:w=\"5000\" w:type=\"pct\"/>\
         <w:tblBorders>\
         <w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"B8B0A4\"/>\
         <w:left w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"B8B0A4\"/>\
         <w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"B8B0A4\"/>\
         <w:right w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"B8B0A4\"/>\
         <w:insideH w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"B8B0A4\"/>\
         <w:insideV w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"B8B0A4\"/>\
         </w:tblBorders></w:tblPr>",
    );
    for (ri, row) in rows.iter().enumerate() {
        body.push_str("<w:tr>");
        for cell in row {
            let bold = if ri == 0 { "<w:b/>" } else { "" };
            body.push_str(&format!(
                "<w:tc><w:tcPr><w:tcW w:w=\"0\" w:type=\"auto\"/></w:tcPr>\
                 <w:p><w:r><w:rPr>{bold}</w:rPr>\
                 <w:t xml:space=\"preserve\">{}</w:t></w:r></w:p></w:tc>",
                xml_escape(cell)
            ));
        }
        body.push_str("</w:tr>");
    }
    body.push_str("</w:tbl>");
    body
}

fn xlsx_sheet_xml(text: &str) -> String {
    let rows = split_table(text);
    let mut body = String::new();
    for (ri, row) in rows.iter().enumerate() {
        let r = ri + 1;
        body.push_str(&format!("<row r=\"{r}\">"));
        for (ci, cell) in row.iter().enumerate() {
            let ref_ = cell_ref(ci, r);
            body.push_str(&format!(
                "<c r=\"{ref_}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
                xml_escape(cell)
            ));
        }
        body.push_str("</row>");
    }
    if body.is_empty() {
        body.push_str("<row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t/></is></c></row>");
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>{body}</sheetData>
</worksheet>"#
    )
}

fn split_table(text: &str) -> Vec<Vec<String>> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }
    if lines.iter().any(|l| is_md_table_row(l.trim())) {
        return split_md_table_lines(&lines);
    }
    let use_tab = lines.iter().any(|l| l.contains('\t'));
    lines
        .into_iter()
        .map(|line| {
            if use_tab {
                line.split('\t').map(|s| s.to_string()).collect()
            } else if line.contains(',') {
                split_csv_line(line)
            } else {
                vec![strip_md_inline(line.trim_start_matches('#').trim())]
            }
        })
        .collect()
}

fn split_md_table_lines(lines: &[&str]) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for line in lines {
        let t = line.trim();
        if is_md_table_row(t) {
            if !is_md_table_sep(t) {
                rows.push(split_md_row(t));
            }
        } else if !t.is_empty() && parse_md_heading(t).is_none() {
            rows.push(vec![strip_md_inline(t)]);
        }
    }
    rows
}

fn split_csv_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if in_q => {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cur.push('"');
                } else {
                    in_q = false;
                }
            }
            '"' => in_q = true,
            ',' if !in_q => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

fn cell_ref(col0: usize, row1: usize) -> String {
    let mut n = col0 + 1;
    let mut letters = String::new();
    while n > 0 {
        n -= 1;
        letters.insert(0, (b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    format!("{letters}{row1}")
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if (c as u32) < 0x20 && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

struct ZipMember<'a> {
    name: &'a str,
    data: &'a [u8],
}

fn zip_store(parts: &[(&str, &[u8])]) -> Vec<u8> {
    let files: Vec<ZipMember<'_>> = parts
        .iter()
        .map(|(n, d)| ZipMember { name: n, data: d })
        .collect();
    let mut out = Vec::new();
    let mut central = Vec::new();
    for f in &files {
        let name = f.name.as_bytes();
        let crc = crc32(f.data);
        let offset = out.len() as u32;
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(f.data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(f.data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name);
        out.extend_from_slice(f.data);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(f.data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(f.data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name);
    }
    let cd_offset = out.len() as u32;
    let cd_size = central.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    let n = files.len() as u16;
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docx_is_zip_and_contains_text() {
        let bytes = package_docx("今日要点\n第一条\n第二条");
        assert_eq!(&bytes[0..2], b"PK");
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("今日要点"));
        assert!(s.contains("第一条"));
    }

    #[test]
    fn xlsx_splits_csv() {
        let bytes = package_xlsx("项目,金额\n差旅,1200");
        assert_eq!(&bytes[0..2], b"PK");
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("差旅"));
        assert!(s.contains("A1"));
        assert!(s.contains("B2"));
    }

    #[test]
    fn doc_upgrades_to_docx() {
        let (p, bytes) = maybe_package(Path::new("/tmp/a.doc"), "hi").unwrap();
        assert_eq!(p.extension().unwrap(), "docx");
        assert_eq!(&bytes[0..2], b"PK");
    }

    #[test]
    fn md_is_not_packaged() {
        assert!(maybe_package(Path::new("outputs/a.md"), "hi").is_none());
    }

    #[test]
    fn cell_refs() {
        assert_eq!(cell_ref(0, 1), "A1");
        assert_eq!(cell_ref(25, 2), "Z2");
        assert_eq!(cell_ref(26, 1), "AA1");
    }

    #[test]
    fn docx_markdown_becomes_heading_and_table() {
        let md =
            "# 今日要点\n\n**摘要**一段话\n\n| 项目 | 金额 |\n| --- | --- |\n| 差旅 | 1200 |\n";
        let bytes = package_docx(md);
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("今日要点"));
        assert!(!s.contains("# 今日要点"), "heading marker must not leak");
        assert!(s.contains("<w:b/>"));
        assert!(s.contains("<w:tbl>"));
        assert!(s.contains("差旅"));
        assert!(!s.contains("| 项目"), "pipe table must not leak as text");
    }

    #[test]
    fn xlsx_markdown_table() {
        let bytes = package_xlsx("| 项目 | 金额 |\n| --- | --- |\n| 差旅 | 1200 |");
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("项目"));
        assert!(s.contains("差旅"));
        assert!(!s.contains("---"));
    }
}
