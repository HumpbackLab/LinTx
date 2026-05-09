#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardField {
    BindPhrase,
}

pub const KEYBOARD_ROWS: usize = 4;
pub const KEYBOARD_MAX_COLS: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardMode {
    Lower,
    Upper,
    Numbers,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyboardAction {
    Insert(char),
    Backspace,
    Space,
    Shift,
    Mode(KeyboardMode),
    CursorLeft,
    CursorRight,
    Cancel,
    Submit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardKey {
    pub label: &'static str,
    pub action: KeyboardAction,
    pub span_half_units: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardRow {
    pub offset_half_units: u8,
    pub keys: Vec<KeyboardKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardFocus {
    Back,
    Ok,
    Key { row: usize, col: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyboardResult {
    None,
    Cancelled,
    Submitted { field: KeyboardField, value: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardOverlay {
    pub field: KeyboardField,
    pub label: String,
    pub buffer: String,
    pub cursor: usize,
    pub mode: KeyboardMode,
    pub selected_row: usize,
    pub selected_col: usize,
    pub focus: KeyboardFocus,
    pub invalid: bool,
    pub max_len: usize,
}

impl KeyboardOverlay {
    pub fn bind_phrase(initial: impl Into<String>) -> Self {
        Self {
            field: KeyboardField::BindPhrase,
            label: "Bind Phrase".to_string(),
            buffer: initial.into(),
            cursor: 0,
            mode: KeyboardMode::Lower,
            selected_row: 0,
            selected_col: 0,
            focus: KeyboardFocus::Key { row: 0, col: 0 },
            invalid: false,
            max_len: 32,
        }
    }

    pub fn rows(&self) -> Vec<Vec<KeyboardKey>> {
        self.layout_rows().into_iter().map(|row| row.keys).collect()
    }

    pub fn layout_rows(&self) -> Vec<KeyboardRow> {
        rows_for_mode(self.mode)
    }

    pub fn selected_key(&self) -> KeyboardKey {
        let rows = self.layout_rows();
        rows[self.selected_row].keys[self.selected_col].clone()
    }

    pub fn move_left(&mut self) {
        match self.focus {
            KeyboardFocus::Back => {}
            KeyboardFocus::Ok => self.focus_top_back(),
            KeyboardFocus::Key { .. } => {
                self.selected_col = self.selected_col.saturating_sub(1);
                self.focus_key(self.selected_row, self.selected_col);
            }
        }
    }

    pub fn move_right(&mut self) {
        match self.focus {
            KeyboardFocus::Back => self.focus_top_ok(),
            KeyboardFocus::Ok => {}
            KeyboardFocus::Key { .. } => {
                let rows = self.layout_rows();
                let max = rows[self.selected_row].keys.len().saturating_sub(1);
                self.selected_col = self.selected_col.saturating_add(1).min(max);
                self.focus_key(self.selected_row, self.selected_col);
            }
        }
    }

    pub fn move_up(&mut self) {
        match self.focus {
            KeyboardFocus::Back | KeyboardFocus::Ok => {}
            KeyboardFocus::Key { row, col } if row == 0 => {
                if col < self.layout_rows()[0].keys.len() / 2 {
                    self.focus_top_back();
                } else {
                    self.focus_top_ok();
                }
            }
            KeyboardFocus::Key { .. } => self.move_vertical(-1),
        }
    }

    pub fn move_down(&mut self) {
        match self.focus {
            KeyboardFocus::Back => self.focus_key(0, 0),
            KeyboardFocus::Ok => {
                let max = self.layout_rows()[0].keys.len().saturating_sub(1);
                self.focus_key(0, max);
            }
            KeyboardFocus::Key { .. } => self.move_vertical(1),
        }
    }

    pub fn select(&mut self, row: usize, col: usize) {
        self.focus_key(row, col);
    }

    fn focus_top_back(&mut self) {
        self.focus = KeyboardFocus::Back;
    }

    fn focus_top_ok(&mut self) {
        self.focus = KeyboardFocus::Ok;
    }

    fn focus_key(&mut self, row: usize, col: usize) {
        let rows = self.layout_rows();
        self.selected_row = row.min(rows.len().saturating_sub(1));
        self.selected_col = col.min(rows[self.selected_row].keys.len().saturating_sub(1));
        self.focus = KeyboardFocus::Key {
            row: self.selected_row,
            col: self.selected_col,
        };
    }

    fn move_vertical(&mut self, delta: isize) {
        let rows = self.layout_rows();
        let row = if delta.is_negative() {
            self.selected_row.saturating_sub(delta.unsigned_abs())
        } else {
            self.selected_row
                .saturating_add(delta as usize)
                .min(rows.len().saturating_sub(1))
        };
        self.selected_row = row;
        self.selected_col = self
            .selected_col
            .min(rows[row].keys.len().saturating_sub(1));
        self.focus_key(self.selected_row, self.selected_col);
    }

    pub fn activate_selected(&mut self) -> KeyboardResult {
        match self.focus {
            KeyboardFocus::Back => self.activate(KeyboardAction::Cancel),
            KeyboardFocus::Ok => self.activate(KeyboardAction::Submit),
            KeyboardFocus::Key { .. } => self.activate(self.selected_key().action),
        }
    }

    pub fn activate(&mut self, action: KeyboardAction) -> KeyboardResult {
        self.invalid = false;
        match action {
            KeyboardAction::Insert(ch) => {
                self.insert_char(ch);
                KeyboardResult::None
            }
            KeyboardAction::Backspace => {
                self.backspace();
                KeyboardResult::None
            }
            KeyboardAction::Space => {
                self.insert_char(' ');
                KeyboardResult::None
            }
            KeyboardAction::Shift => {
                self.mode = match self.mode {
                    KeyboardMode::Lower => KeyboardMode::Upper,
                    KeyboardMode::Upper => KeyboardMode::Lower,
                    KeyboardMode::Numbers => KeyboardMode::Upper,
                };
                self.clamp_selection();
                KeyboardResult::None
            }
            KeyboardAction::Mode(mode) => {
                self.mode = mode;
                self.clamp_selection();
                KeyboardResult::None
            }
            KeyboardAction::CursorLeft => {
                self.cursor = self.cursor.saturating_sub(1);
                KeyboardResult::None
            }
            KeyboardAction::CursorRight => {
                self.cursor = self.cursor.saturating_add(1).min(self.buffer.len());
                KeyboardResult::None
            }
            KeyboardAction::Cancel => KeyboardResult::Cancelled,
            KeyboardAction::Submit => KeyboardResult::Submitted {
                field: self.field,
                value: self.buffer.clone(),
            },
        }
    }

    pub fn mark_invalid(&mut self) {
        self.invalid = true;
    }

    fn clamp_selection(&mut self) {
        let rows = self.rows();
        self.selected_row = self.selected_row.min(rows.len().saturating_sub(1));
        self.selected_col = self
            .selected_col
            .min(rows[self.selected_row].len().saturating_sub(1));
        self.focus_key(self.selected_row, self.selected_col);
    }

    fn insert_char(&mut self, ch: char) {
        if self.buffer.len().saturating_add(ch.len_utf8()) > self.max_len {
            return;
        }
        self.buffer.insert(self.cursor.min(self.buffer.len()), ch);
        self.cursor = self
            .cursor
            .saturating_add(ch.len_utf8())
            .min(self.buffer.len());
    }

    fn backspace(&mut self) {
        if self.cursor == 0 || self.buffer.is_empty() {
            return;
        }
        let remove_at = self.cursor.saturating_sub(1);
        self.buffer.remove(remove_at);
        self.cursor = remove_at;
    }
}

pub fn validate_bind_phrase(value: &str) -> Result<(), &'static str> {
    if value.is_empty() {
        return Err("Bind phrase is empty");
    }
    if value.len() > 32 {
        return Err("Bind phrase is too long");
    }
    if value
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        Ok(())
    } else {
        Err("Use a-z, 0-9, - or _")
    }
}

fn key(label: &'static str, action: KeyboardAction) -> KeyboardKey {
    wide_key(label, action, 2)
}

fn wide_key(label: &'static str, action: KeyboardAction, span_half_units: u8) -> KeyboardKey {
    KeyboardKey {
        label,
        action,
        span_half_units,
    }
}

fn char_key(ch: char) -> KeyboardKey {
    let label = match ch {
        'a' => "a",
        'b' => "b",
        'c' => "c",
        'd' => "d",
        'e' => "e",
        'f' => "f",
        'g' => "g",
        'h' => "h",
        'i' => "i",
        'j' => "j",
        'k' => "k",
        'l' => "l",
        'm' => "m",
        'n' => "n",
        'o' => "o",
        'p' => "p",
        'q' => "q",
        'r' => "r",
        's' => "s",
        't' => "t",
        'u' => "u",
        'v' => "v",
        'w' => "w",
        'x' => "x",
        'y' => "y",
        'z' => "z",
        'A' => "A",
        'B' => "B",
        'C' => "C",
        'D' => "D",
        'E' => "E",
        'F' => "F",
        'G' => "G",
        'H' => "H",
        'I' => "I",
        'J' => "J",
        'K' => "K",
        'L' => "L",
        'M' => "M",
        'N' => "N",
        'O' => "O",
        'P' => "P",
        'Q' => "Q",
        'R' => "R",
        'S' => "S",
        'T' => "T",
        'U' => "U",
        'V' => "V",
        'W' => "W",
        'X' => "X",
        'Y' => "Y",
        'Z' => "Z",
        '0' => "0",
        '1' => "1",
        '2' => "2",
        '3' => "3",
        '4' => "4",
        '5' => "5",
        '6' => "6",
        '7' => "7",
        '8' => "8",
        '9' => "9",
        '-' => "-",
        '_' => "_",
        '.' => ".",
        ',' => ",",
        '/' => "/",
        ':' => ":",
        ';' => ";",
        '@' => "@",
        '#' => "#",
        '+' => "+",
        '&' => "&",
        '*' => "*",
        '=' => "=",
        '%' => "%",
        '!' => "!",
        '?' => "?",
        '<' => "<",
        '>' => ">",
        '\\' => "\\",
        '$' => "$",
        '(' => "(",
        ')' => ")",
        '{' => "{",
        '}' => "}",
        '[' => "[",
        ']' => "]",
        '"' => "\"",
        '\'' => "'",
        _ => "?",
    };
    key(label, KeyboardAction::Insert(ch))
}

fn row(offset_half_units: u8, keys: Vec<KeyboardKey>) -> KeyboardRow {
    KeyboardRow {
        offset_half_units,
        keys,
    }
}

fn rows_for_mode(mode: KeyboardMode) -> Vec<KeyboardRow> {
    match mode {
        KeyboardMode::Lower => letter_rows(false),
        KeyboardMode::Upper => letter_rows(true),
        KeyboardMode::Numbers => vec![
            row(0, "1234567890".chars().map(char_key).collect()),
            row(0, "+&/*=%!?#<>".chars().map(char_key).collect()),
            row(0, "\\@$(){}[];\"'".chars().map(char_key).collect()),
            row(
                2,
                vec![
                    wide_key("abc", KeyboardAction::Mode(KeyboardMode::Lower), 3),
                    key("<", KeyboardAction::CursorLeft),
                    wide_key("space", KeyboardAction::Space, 6),
                    key(">", KeyboardAction::CursorRight),
                    wide_key("done", KeyboardAction::Submit, 4),
                ],
            ),
        ],
    }
}

fn letter_rows(upper: bool) -> Vec<KeyboardRow> {
    let row1 = if upper { "QWERTYUIOP" } else { "qwertyuiop" };
    let row2 = if upper { "ASDFGHJKL" } else { "asdfghjkl" };
    let row3 = if upper { "ZXCVBNM" } else { "zxcvbnm" };
    vec![
        row(0, row1.chars().map(char_key).collect()),
        row(1, row2.chars().map(char_key).collect()),
        row(
            0,
            std::iter::once(wide_key("ABC", KeyboardAction::Shift, 3))
                .chain(row3.chars().map(char_key))
                .chain(std::iter::once(wide_key(
                    "del",
                    KeyboardAction::Backspace,
                    3,
                )))
                .collect(),
        ),
        row(
            2,
            vec![
                wide_key("123", KeyboardAction::Mode(KeyboardMode::Numbers), 3),
                key("<", KeyboardAction::CursorLeft),
                wide_key("space", KeyboardAction::Space, 6),
                key(">", KeyboardAction::CursorRight),
                wide_key("done", KeyboardAction::Submit, 4),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        validate_bind_phrase, KeyboardAction, KeyboardMode, KeyboardOverlay, KeyboardResult,
    };

    #[test]
    fn keyboard_moves_selection_across_rows_and_clamps_column() {
        let mut keyboard = KeyboardOverlay::bind_phrase("");

        keyboard.move_right();
        keyboard.move_down();
        keyboard.move_down();
        keyboard.move_down();
        keyboard.move_right();
        keyboard.move_right();
        keyboard.move_right();
        keyboard.move_right();
        keyboard.move_right();

        assert_eq!(keyboard.selected_row, 3);
        assert_eq!(keyboard.selected_col, 4);
        assert_eq!(keyboard.selected_key().label, "done");
    }

    #[test]
    fn keyboard_inserts_at_cursor_and_backspaces() {
        let mut keyboard = KeyboardOverlay::bind_phrase("ac");
        keyboard.cursor = 1;

        keyboard.activate(KeyboardAction::Insert('b'));
        keyboard.activate(KeyboardAction::Backspace);

        assert_eq!(keyboard.buffer, "ac");
        assert_eq!(keyboard.cursor, 1);
    }

    #[test]
    fn keyboard_switches_modes_and_submit_returns_value() {
        let mut keyboard = KeyboardOverlay::bind_phrase("");

        keyboard.activate(KeyboardAction::Mode(KeyboardMode::Numbers));
        keyboard.activate(KeyboardAction::Insert('1'));
        let result = keyboard.activate(KeyboardAction::Submit);

        assert_eq!(keyboard.mode, KeyboardMode::Numbers);
        assert_eq!(
            result,
            KeyboardResult::Submitted {
                field: super::KeyboardField::BindPhrase,
                value: "1".to_string()
            }
        );
    }

    #[test]
    fn bind_phrase_validation_rejects_invalid_text_without_changing_buffer() {
        let mut keyboard = KeyboardOverlay::bind_phrase("ABC");

        assert!(validate_bind_phrase(&keyboard.buffer).is_err());
        keyboard.mark_invalid();

        assert!(keyboard.invalid);
        assert_eq!(keyboard.buffer, "ABC");
    }

    #[test]
    fn bind_phrase_validation_accepts_allowed_text() {
        assert_eq!(validate_bind_phrase("abc-123_def"), Ok(()));
    }

    #[test]
    fn keyboard_navigation_can_focus_top_actions() {
        let mut keyboard = KeyboardOverlay::bind_phrase("");

        keyboard.move_up();
        assert_eq!(keyboard.focus, super::KeyboardFocus::Back);

        keyboard.move_right();
        assert_eq!(keyboard.focus, super::KeyboardFocus::Ok);

        keyboard.move_down();
        assert_eq!(keyboard.focus, super::KeyboardFocus::Key { row: 0, col: 9 });
    }

    #[test]
    fn letter_layout_offsets_and_wide_keys_match_visual_grid() {
        let keyboard = KeyboardOverlay::bind_phrase("");
        let rows = keyboard.layout_rows();

        assert_eq!(rows[1].offset_half_units, 1);
        assert_eq!(rows[2].keys[0].label, "ABC");
        assert_eq!(rows[2].keys[0].span_half_units, 3);
        assert_eq!(rows[2].keys.last().unwrap().label, "del");
        assert_eq!(rows[2].keys.last().unwrap().span_half_units, 3);
        assert_eq!(rows[3].offset_half_units, 2);
        assert_eq!(rows[3].keys[2].label, "space");
        assert_eq!(rows[3].keys[2].span_half_units, 6);
        assert_eq!(rows[3].keys[4].label, "done");
        assert_eq!(rows[3].keys[4].span_half_units, 4);
    }

    #[test]
    fn number_layout_keeps_space_and_action_keys_wide() {
        let mut keyboard = KeyboardOverlay::bind_phrase("");
        keyboard.activate(KeyboardAction::Mode(KeyboardMode::Numbers));
        let rows = keyboard.layout_rows();

        assert_eq!(
            rows[1].keys.iter().map(|key| key.label).collect::<Vec<_>>(),
            vec!["+", "&", "/", "*", "=", "%", "!", "?", "#", "<", ">"]
        );
        assert_eq!(
            rows[2].keys.iter().map(|key| key.label).collect::<Vec<_>>(),
            vec!["\\", "@", "$", "(", ")", "{", "}", "[", "]", ";", "\"", "'"]
        );
        assert_eq!(rows[3].offset_half_units, 2);
        assert_eq!(rows[3].keys[0].label, "abc");
        assert_eq!(rows[3].keys[0].span_half_units, 3);
        assert_eq!(rows[3].keys[2].label, "space");
        assert_eq!(rows[3].keys[2].span_half_units, 6);
        assert_eq!(rows[3].keys[4].label, "done");
        assert_eq!(rows[3].keys[4].span_half_units, 4);
    }
}
