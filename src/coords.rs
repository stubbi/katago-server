//! Go board coordinates in the GTP/KataGo letter-number style ("D4", "Q16").
//!
//! Columns run A..Z skipping "I"; rows run 1..=size from the bottom.

/// Largest board KataGo supports along either axis.
pub const MAX_BOARD_SIZE: u8 = 25;
/// Smallest board KataGo supports along either axis.
pub const MIN_BOARD_SIZE: u8 = 2;

const COLUMN_LETTERS: &[u8; 25] = b"ABCDEFGHJKLMNOPQRSTUVWXYZ";

/// A parsed vertex: either a pass or a zero-based point on the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vertex {
    /// A pass move.
    Pass,
    /// A board point, zero-based from the bottom-left corner.
    Point {
        /// Zero-based column (A = 0).
        col: u8,
        /// Zero-based row (1 = 0).
        row: u8,
    },
}

/// Returns the letter for a zero-based column index, skipping "I".
pub fn column_letter(col: u8) -> Option<char> {
    COLUMN_LETTERS.get(usize::from(col)).map(|b| *b as char)
}

/// Returns the last column letter for a board of the given width.
pub fn last_column_letter(width: u8) -> Option<char> {
    width.checked_sub(1).and_then(column_letter)
}

/// Parses a vertex such as `"D4"`, `"d4"` or `"pass"` for a board of the given size.
///
/// Returns `None` if the text is not a coordinate or lies outside the board.
pub fn parse_vertex(text: &str, width: u8, height: u8) -> Option<Vertex> {
    let text = text.trim();
    if text.eq_ignore_ascii_case("pass") {
        return Some(Vertex::Pass);
    }
    let mut chars = text.chars();
    let letter = chars.next()?.to_ascii_uppercase();
    if !letter.is_ascii_alphabetic() {
        return None;
    }
    let col = COLUMN_LETTERS
        .iter()
        .position(|b| *b as char == letter)
        .and_then(|i| u8::try_from(i).ok())?;
    let row_text = chars.as_str();
    if row_text.is_empty() || !row_text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let row_one_based: u8 = row_text.parse().ok()?;
    if row_one_based == 0 || col >= width || row_one_based > height {
        return None;
    }
    Some(Vertex::Point {
        col,
        row: row_one_based - 1,
    })
}

/// Formats a vertex back into canonical text (`"D4"`, `"pass"`).
pub fn format_vertex(vertex: Vertex) -> String {
    match vertex {
        Vertex::Pass => "pass".to_owned(),
        Vertex::Point { col, row } => {
            let letter = column_letter(col).unwrap_or('?');
            format!("{letter}{}", u16::from(row) + 1)
        }
    }
}

/// Validates and canonicalises a coordinate for the given board.
pub fn normalize(text: &str, width: u8, height: u8) -> Option<String> {
    parse_vertex(text, width, height).map(format_vertex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_coordinates() {
        assert_eq!(
            parse_vertex("A1", 19, 19),
            Some(Vertex::Point { col: 0, row: 0 })
        );
        assert_eq!(
            parse_vertex("T19", 19, 19),
            Some(Vertex::Point { col: 18, row: 18 })
        );
        assert_eq!(
            parse_vertex("d4", 19, 19),
            Some(Vertex::Point { col: 3, row: 3 })
        );
        // J is the 9th column because I is skipped.
        assert_eq!(
            parse_vertex("J9", 9, 9),
            Some(Vertex::Point { col: 8, row: 8 })
        );
    }

    #[test]
    fn parses_pass_case_insensitively() {
        assert_eq!(parse_vertex("pass", 9, 9), Some(Vertex::Pass));
        assert_eq!(parse_vertex("PASS", 9, 9), Some(Vertex::Pass));
        assert_eq!(parse_vertex(" Pass ", 9, 9), Some(Vertex::Pass));
    }

    #[test]
    fn rejects_out_of_range_and_garbage() {
        assert_eq!(parse_vertex("I5", 19, 19), None);
        assert_eq!(parse_vertex("U1", 19, 19), None);
        assert_eq!(parse_vertex("A20", 19, 19), None);
        assert_eq!(parse_vertex("A0", 19, 19), None);
        assert_eq!(parse_vertex("R4", 9, 9), None);
        assert_eq!(parse_vertex("K1", 9, 9), None);
        assert_eq!(parse_vertex("", 19, 19), None);
        assert_eq!(parse_vertex("4D", 19, 19), None);
        assert_eq!(parse_vertex("D", 19, 19), None);
        assert_eq!(parse_vertex("D4x", 19, 19), None);
        assert_eq!(parse_vertex("D-4", 19, 19), None);
        assert!(parse_vertex("Z25", 25, 25).is_some());
        assert_eq!(parse_vertex("Z25", 24, 25), None);
    }

    #[test]
    fn rectangular_boards_use_both_dimensions() {
        assert!(parse_vertex("N5", 13, 5).is_some());
        assert!(parse_vertex("N6", 13, 5).is_none());
        assert!(parse_vertex("O5", 13, 5).is_none());
    }

    #[test]
    fn normalizes_to_canonical_text() {
        assert_eq!(normalize("d4", 19, 19).as_deref(), Some("D4"));
        assert_eq!(normalize("PASS", 19, 19).as_deref(), Some("pass"));
        assert_eq!(normalize("q16 ", 19, 19).as_deref(), Some("Q16"));
        assert_eq!(normalize("I1", 19, 19), None);
    }

    #[test]
    fn column_letters_skip_i() {
        assert_eq!(column_letter(0), Some('A'));
        assert_eq!(column_letter(7), Some('H'));
        assert_eq!(column_letter(8), Some('J'));
        assert_eq!(column_letter(24), Some('Z'));
        assert_eq!(column_letter(25), None);
        assert_eq!(last_column_letter(19), Some('T'));
        assert_eq!(last_column_letter(9), Some('J'));
        assert_eq!(last_column_letter(0), None);
    }
}
