//! Files into rows of cells: xlsx through `calamine`, CSV through `csv`.

use std::io::Cursor;

use calamine::{DataRef, Reader, Xlsx, open_workbook_from_rs};
use chrono::NaiveDateTime;

use super::{
    MAX_CELL_CHARS, MAX_COLUMNS, MAX_ROWS, MAX_SHARED_STRINGS, MAX_TEXT_BYTES,
    MAX_XLSX_EXPANDED_BYTES, ParseError,
};

/// One cell, as the file wrote it. Numbers keep their shortest decimal text
/// ("-41.76"), so no amount ever goes through float arithmetic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cell {
    Empty,
    Text(String),
    Number(String),
    DateTime(NaiveDateTime),
}

impl Cell {
    /// The cell as text: what a person would read in it, trimmed and cut
    /// at `MAX_CELL_CHARS`. Dates as ISO.
    pub fn text(&self) -> String {
        match self {
            Cell::Empty => String::new(),
            Cell::Text(text) | Cell::Number(text) => {
                text.trim().chars().take(MAX_CELL_CHARS).collect()
            }
            Cell::DateTime(moment) => moment.to_string(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Cell::Empty => true,
            Cell::Text(text) => text.trim().is_empty(),
            Cell::Number(_) | Cell::DateTime(_) => false,
        }
    }
}

/// The rows of a CSV file. Row `i` is line `i + 1`, blank lines included,
/// and a record spanning lines sits at its first line. A UTF-8 byte order
/// mark is dropped; a file that is not valid UTF-8 is read as Windows-1252
/// (what Italian banks use when it is not UTF-8). The delimiter is the one
/// of `,`, `;` and tab that appears most in the first line (L5). Reading
/// stops at the row and column limits.
pub fn read_csv(bytes: &[u8]) -> Result<Vec<Vec<Cell>>, ParseError> {
    let text = decode(bytes);
    let delimiter = sniff_delimiter(&text);
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(text.as_bytes());
    let mut rows: Vec<Vec<Cell>> = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|error| ParseError::Unreadable(error.to_string()))?;
        if record.len() > MAX_COLUMNS {
            return Err(ParseError::TooManyColumns { limit: MAX_COLUMNS });
        }
        let line = record
            .position()
            .map_or(rows.len() + 1, |p| line_of(&text, p.line(), p.byte()));
        if line > MAX_ROWS {
            return Err(ParseError::TooManyRows { limit: MAX_ROWS });
        }
        // Blank lines are skipped by the reader; keep their places.
        rows.resize(line.saturating_sub(1).max(rows.len()), Vec::new());
        rows.push(record.iter().map(text_cell).collect());
    }
    Ok(rows)
}

/// The line a record starts on. The csv crate reports the position before
/// the blank lines it skipped, so those are added back.
fn line_of(text: &str, line: u64, byte: u64) -> usize {
    let start = usize::try_from(byte).unwrap_or(usize::MAX);
    let skipped = text
        .as_bytes()
        .get(start..)
        .unwrap_or_default()
        .iter()
        .take_while(|b| matches!(b, b'\n' | b'\r'))
        .filter(|b| **b == b'\n')
        .count();
    usize::try_from(line)
        .unwrap_or(usize::MAX)
        .saturating_add(skipped)
}

fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned(),
    }
}

fn sniff_delimiter(text: &str) -> u8 {
    let first = text.lines().next().unwrap_or("");
    let count = |delimiter: u8| first.bytes().filter(|b| *b == delimiter).count();
    // The most frequent wins; on a tie, or when none appears (a one-column
    // file), the earlier in this list.
    let mut best = b',';
    for delimiter in *b",;\t" {
        if count(delimiter) > count(best) {
            best = delimiter;
        }
    }
    best
}

fn text_cell(value: &str) -> Cell {
    if value.trim().is_empty() {
        Cell::Empty
    } else {
        Cell::Text(value.to_string())
    }
}

/// The rows of one sheet of an xlsx file: `sheet`, or the first one. Row
/// `i` is spreadsheet row `i + 1` and column `j` is column `j + 1`, so
/// reasons can name the row the owner sees.
///
/// Cells are read one by one and the limits are checked before anything is
/// stored, so a tiny file declaring a cell at XFD1048576 or millions of
/// copies of one shared string is refused instead of exhausting memory.
pub fn read_xlsx(bytes: &[u8], sheet: Option<&str>) -> Result<Vec<Vec<Cell>>, ParseError> {
    check_archive(bytes)?;
    // A backstop, not the plan: calamine does unchecked arithmetic on cell
    // references, which panics in debug builds on a crafted one such as
    // "A99999999999". `catch_unwind` turns that panic into an error, like
    // a PHP `catch (\Throwable)` around a library call, or Go's `recover`.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read_sheet(bytes, sheet)))
        .unwrap_or_else(|_| Err(ParseError::Unreadable("malformed spreadsheet".to_string())))
}

fn read_sheet(bytes: &[u8], sheet: Option<&str>) -> Result<Vec<Vec<Cell>>, ParseError> {
    let unreadable = |error: calamine::XlsxError| ParseError::Unreadable(error.to_string());
    let mut workbook: Xlsx<_> = open_workbook_from_rs(Cursor::new(bytes)).map_err(unreadable)?;
    let names = workbook.sheet_names();
    let name = match sheet {
        Some(name) if names.iter().any(|n| n == name) => name.to_string(),
        Some(name) => return Err(ParseError::NoSheet(name.to_string())),
        None => names
            .into_iter()
            .next()
            .ok_or_else(|| ParseError::Unreadable("the workbook has no sheets".to_string()))?,
    };
    let mut reader = workbook.worksheet_cells_reader(&name).map_err(unreadable)?;
    let mut rows: Vec<Vec<Cell>> = Vec::new();
    let mut text_bytes = 0usize;
    while let Some(found) = reader.next_cell().map_err(unreadable)? {
        let (row, column) = found.get_position();
        let (row, column) = (row as usize, column as usize);
        if row >= MAX_ROWS {
            return Err(ParseError::TooManyRows { limit: MAX_ROWS });
        }
        if column >= MAX_COLUMNS {
            return Err(ParseError::TooManyColumns { limit: MAX_COLUMNS });
        }
        // Counted before trimming: whitespace costs time to scan too.
        text_bytes = text_bytes.saturating_add(raw_len(found.get_value()));
        if text_bytes > MAX_TEXT_BYTES {
            return Err(ParseError::TooMuchText {
                limit: MAX_TEXT_BYTES,
            });
        }
        let value = xlsx_cell_value(found.get_value());
        if rows.len() <= row {
            rows.resize(row + 1, Vec::new());
        }
        let cells = &mut rows[row];
        if cells.len() <= column {
            cells.resize(column + 1, Cell::Empty);
        }
        cells[column] = value;
    }
    Ok(rows)
}

/// Checks the archive before calamine opens it: the declared uncompressed
/// sizes (zip 9.5 refuses an entry that expands past its declared size, so
/// the total holds), and the shared-string count calamine reserves memory
/// for up front. Every XML entry's first element is read with an XML
/// parser, so quoting, padding or another entry name cannot hide the count.
fn check_archive(bytes: &[u8]) -> Result<(), ParseError> {
    let unreadable = |error: zip::result::ZipError| ParseError::Unreadable(error.to_string());
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(unreadable)?;
    let mut total: u64 = 0;
    let mut xml_entries = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index_raw(index).map_err(unreadable)?;
        total = total.saturating_add(entry.size());
        if entry
            .name()
            .map_err(unreadable)?
            .to_lowercase()
            .ends_with(".xml")
        {
            xml_entries.push(index);
        }
    }
    if total > MAX_XLSX_EXPANDED_BYTES {
        return Err(ParseError::TooLargeExpanded {
            limit: MAX_XLSX_EXPANDED_BYTES,
        });
    }
    for index in xml_entries {
        let entry = archive.by_index(index).map_err(unreadable)?;
        if shared_string_count(std::io::BufReader::new(entry)) > MAX_SHARED_STRINGS {
            return Err(ParseError::Unreadable(
                "too many shared strings".to_string(),
            ));
        }
    }
    Ok(())
}

/// The `uniqueCount` of the first `<sst>` element anywhere in the entry
/// (calamine looks for that one, not only the root), 0 when there is none.
/// The whole entry may be read, which the 50 MB declared total bounds.
/// Unreadable XML or a count that does not parse counts as too many.
fn shared_string_count(entry: impl std::io::BufRead) -> u64 {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_reader(entry);
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        let element = match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(element) | Event::Empty(element)) => element,
            Ok(Event::Eof) => return 0,
            Ok(_) => continue,
            Err(_) => return u64::MAX,
        };
        if element.local_name().as_ref() != "sst" {
            continue;
        }
        for attribute in element.attributes() {
            let Ok(attribute) = attribute else {
                return u64::MAX;
            };
            if attribute.key.local_name().as_ref() == "uniqueCount" {
                return attribute.value.trim().parse().unwrap_or(u64::MAX);
            }
        }
        return 0;
    }
}

/// The bytes of text a cell holds as written; a few for anything else.
fn raw_len(data: &DataRef<'_>) -> usize {
    const SMALL: usize = 8;
    match data {
        DataRef::SharedString(text) => text.len(),
        DataRef::String(text) | DataRef::DateTimeIso(text) | DataRef::DurationIso(text) => {
            text.len()
        }
        _ => SMALL,
    }
}

/// The last day Excel can show (9999-12-31): later serials, and negative
/// ones, cannot be dates, and calamine's conversion would panic on huge ones.
const LAST_EXCEL_SERIAL: f64 = 2_958_465.0;

fn xlsx_cell_value(data: &DataRef<'_>) -> Cell {
    match data {
        DataRef::Empty => Cell::Empty,
        DataRef::SharedString(text) => text_cell(text),
        DataRef::String(text) | DataRef::DateTimeIso(text) | DataRef::DurationIso(text) => {
            text_cell(text)
        }
        // `{}` prints a float's shortest exact decimal ("-41.76", "1457"),
        // never scientific notation, so the amount parser sees what the
        // owner typed.
        DataRef::Float(number) => Cell::Number(format!("{number}")),
        DataRef::Int(number) => Cell::Number(number.to_string()),
        DataRef::Bool(value) => Cell::Text(value.to_string()),
        DataRef::DateTime(moment) => {
            let serial = moment.as_f64();
            if (0.0..=LAST_EXCEL_SERIAL).contains(&serial) {
                moment.as_datetime().map_or(Cell::Empty, Cell::DateTime)
            } else {
                Cell::Empty
            }
        }
        DataRef::Error(error) => Cell::Text(format!("#{error:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal xlsx with `sheet` as its only worksheet and, when given,
    /// `shared` as its shared strings.
    fn xlsx(sheet: &str, shared: Option<&str>) -> Vec<u8> {
        use std::io::Write;
        let mut out = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut out);
        let options = zip::write::SimpleFileOptions::default();
        let mut files = vec![
            ("[Content_Types].xml", r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#.to_string()),
            ("_rels/.rels", r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.to_string()),
            ("xl/workbook.xml", r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#.to_string()),
            ("xl/_rels/workbook.xml.rels", r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#.to_string()),
            ("xl/worksheets/sheet1.xml", format!(r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{sheet}</sheetData></worksheet>"#)),
        ];
        if let Some(shared) = shared {
            files.push(("xl/sharedStrings.xml", shared.to_string()));
        }
        for (name, body) in files {
            zip.start_file(name, options).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        out.into_inner()
    }

    #[test]
    fn a_minimal_workbook_reads_with_row_numbers_kept() {
        let rows = read_xlsx(&xlsx(r#"<row r="3"><c r="B3" t="inlineStr"><is><t>Data</t></is></c><c r="C3"><v>-41.76</v></c></row>"#, None), None).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[2][1], Cell::Text("Data".to_string()));
        assert_eq!(rows[2][2], Cell::Number("-41.76".to_string()));
    }

    #[test]
    fn a_cell_far_down_is_refused_before_anything_is_allocated() {
        let sheet = r#"<row r="1"><c r="A1"><v>1</v></c></row><row r="1048576"><c r="XFD1048576"><v>1</v></c></row>"#;
        assert_eq!(
            read_xlsx(&xlsx(sheet, None), None),
            Err(ParseError::TooManyRows {
                limit: super::super::MAX_ROWS
            })
        );
    }

    #[test]
    fn a_cell_far_right_is_refused() {
        let sheet = r#"<row r="1"><c r="XFD1"><v>1</v></c></row>"#;
        assert!(matches!(
            read_xlsx(&xlsx(sheet, None), None),
            Err(ParseError::TooManyColumns { .. })
        ));
    }

    #[test]
    fn shared_string_counts_are_read_like_an_xml_parser_reads_them() {
        // Single quotes, more than 4 KB of padding before the element, and
        // an entry under another name are all still caught.
        let padding = " ".repeat(8_192);
        let tricky = format!(
            "<?xml version='1.0'?><!-- c -->{padding}<sst xmlns='http://schemas.openxmlformats.org/spreadsheetml/2006/main' uniqueCount = '99999999999999'><si><t>x</t></si></sst>"
        );
        assert!(matches!(
            read_xlsx(&xlsx("", Some(&tricky)), None),
            Err(ParseError::Unreadable(_))
        ));
    }

    #[test]
    fn a_shared_string_table_inside_another_element_is_still_checked() {
        // calamine reserves for the first <sst> anywhere, not only the root.
        let wrapped = r#"<?xml version="1.0"?><wrapper><sst uniqueCount="99999999999999"><si><t>x</t></si></sst></wrapper>"#;
        assert!(matches!(
            read_xlsx(&xlsx("", Some(wrapped)), None),
            Err(ParseError::Unreadable(_))
        ));
    }

    #[test]
    fn a_small_shared_string_count_is_fine() {
        let shared = r#"<?xml version="1.0"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="1" uniqueCount="1"><si><t>Data</t></si></sst>"#;
        let sheet = r#"<row r="1"><c r="A1" t="s"><v>0</v></c></row>"#;
        let rows = read_xlsx(&xlsx(sheet, Some(shared)), None).unwrap();
        assert_eq!(rows[0][0], Cell::Text("Data".to_string()));
    }

    #[test]
    fn a_huge_shared_string_count_is_refused_before_calamine_reserves_it() {
        let shared = r#"<?xml version="1.0"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="1" uniqueCount="99999999999999"><si><t>x</t></si></sst>"#;
        assert!(matches!(
            read_xlsx(&xlsx("", Some(shared)), None),
            Err(ParseError::Unreadable(_))
        ));
    }

    #[test]
    fn a_date_cell_out_of_range_is_empty_not_a_panic() {
        use calamine::{ExcelDateTime, ExcelDateTimeType};
        let moment = |value| {
            xlsx_cell_value(&calamine::DataRef::DateTime(ExcelDateTime::new(
                value,
                ExcelDateTimeType::DateTime,
                false,
            )))
        };
        assert_eq!(moment(-1e12), Cell::Empty);
        assert_eq!(moment(1e12), Cell::Empty);
        assert!(matches!(moment(46351.0), Cell::DateTime(_)));
    }

    #[test]
    fn csv_row_numbers_survive_blank_lines_and_multi_line_fields() {
        let rows = read_csv(b"a;b\n\n1;\"two\nlines\"\n3;x\n").unwrap();
        assert_eq!(rows[2][0], Cell::Text("1".to_string()));
        assert_eq!(rows[4][0], Cell::Text("3".to_string()));
    }

    #[test]
    fn csv_row_numbers_with_windows_line_endings() {
        let rows = read_csv(b"a;b\r\n\r\n1;x\r\n").unwrap();
        assert_eq!(rows[2][0], Cell::Text("1".to_string()));
    }

    #[test]
    fn whitespace_counts_toward_the_text_budget() {
        // One shared string of 1 MB of spaces, used by eleven cells.
        let spaces = " ".repeat(1024 * 1024);
        let shared = format!(
            r#"<?xml version="1.0"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" uniqueCount="1"><si><t xml:space="preserve">{spaces}</t></si></sst>"#
        );
        let cells: String = (1..=11)
            .map(|r| format!(r#"<row r="{r}"><c r="A{r}" t="s"><v>0</v></c></row>"#))
            .collect();
        assert!(matches!(
            read_xlsx(&xlsx(&cells, Some(&shared)), None),
            Err(ParseError::TooMuchText { .. })
        ));
    }

    #[test]
    fn csv_limits_rows_and_columns_while_reading() {
        let many_rows = "a\n".repeat(super::super::MAX_ROWS + 1);
        assert!(matches!(
            read_csv(many_rows.as_bytes()),
            Err(ParseError::TooManyRows { .. })
        ));
        let wide = ";".repeat(MAX_COLUMNS);
        assert!(matches!(
            read_csv(wide.as_bytes()),
            Err(ParseError::TooManyColumns { .. })
        ));
    }

    #[test]
    fn long_cells_are_clipped() {
        let long = Cell::Text("x".repeat(MAX_CELL_CHARS + 10));
        assert_eq!(long.text().chars().count(), MAX_CELL_CHARS);
    }

    fn texts(rows: &[Vec<Cell>]) -> Vec<Vec<String>> {
        rows.iter()
            .map(|row| row.iter().map(Cell::text).collect())
            .collect()
    }

    #[test]
    fn csv_with_semicolons_and_a_bom() {
        let rows = read_csv(b"\xEF\xBB\xBFData;Importo\n05/01/2026;-1.234,56\n").unwrap();
        assert_eq!(
            texts(&rows),
            vec![vec!["Data", "Importo"], vec!["05/01/2026", "-1.234,56"]]
        );
    }

    #[test]
    fn csv_with_commas_and_quotes() {
        let rows = read_csv(b"Order ID,Product Name\n171-1,\"Moka, nera\"\n").unwrap();
        assert_eq!(rows[1][1], Cell::Text("Moka, nera".to_string()));
    }

    #[test]
    fn csv_in_windows_1252() {
        // "Caffè" with è as the single byte 0xE8.
        let rows = read_csv(b"Descrizione\tImporto\nCaff\xE8\t1\n").unwrap();
        assert_eq!(rows[1][0], Cell::Text("Caffè".to_string()));
    }

    #[test]
    fn csv_empty_cells_and_short_rows() {
        let rows = read_csv(b"a;b;c\n1;;\n\n2\n").unwrap();
        assert_eq!(rows[1][1], Cell::Empty);
        // The blank line stays an empty row, so row numbers match the file.
        assert!(rows[2].is_empty());
        assert_eq!(rows[3].len(), 1);
    }

    #[test]
    fn xlsx_rejects_a_zip_that_is_not_a_workbook() {
        assert!(matches!(
            read_xlsx(b"PK nonsense", None),
            Err(ParseError::Unreadable(_))
        ));
    }

    #[test]
    fn a_one_column_csv_uses_the_comma() {
        assert_eq!(sniff_delimiter("Descrizione\nx"), b',');
        assert_eq!(sniff_delimiter("a;b,c"), b',');
        assert_eq!(sniff_delimiter("a;b;c,d"), b';');
    }

    #[test]
    fn cells_know_when_they_are_empty() {
        assert!(Cell::Text("  ".to_string()).is_empty());
        assert!(!Cell::Number("0".to_string()).is_empty());
    }
}
