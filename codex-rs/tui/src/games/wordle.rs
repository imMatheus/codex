use std::collections::HashSet;
use std::sync::OnceLock;

use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;

use super::GameWidget;

const MAX_GUESSES: usize = 6;
const WORD_LEN: usize = 5;

static WORD_LIST: OnceLock<Vec<&'static str>> = OnceLock::new();
static VALID_GUESSES: OnceLock<HashSet<&'static str>> = OnceLock::new();

#[derive(Clone, Copy, PartialEq, Eq)]
enum LetterResult {
    /// Correct letter, correct position.
    Correct,
    /// Correct letter, wrong position.
    Present,
    /// Letter not in word.
    Absent,
}

struct Guess {
    word: [char; WORD_LEN],
    results: [LetterResult; WORD_LEN],
}

pub(crate) struct WordleGame {
    target: [char; WORD_LEN],
    guesses: Vec<Guess>,
    current_input: Vec<char>,
    won: bool,
    game_over: bool,
    rng: u32,
    message: Option<&'static str>,
}

impl WordleGame {
    pub fn new() -> Self {
        let mut game = Self {
            target: ['a'; WORD_LEN],
            guesses: Vec::new(),
            current_input: Vec::new(),
            won: false,
            game_over: false,
            rng: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u32)
                .unwrap_or(0xDEAD)
                | 1,
            message: None,
        };
        game.pick_word();
        game
    }

    fn rand_u32(&mut self) -> u32 {
        let mut s = self.rng;
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        self.rng = s;
        s
    }

    fn pick_word(&mut self) {
        let words = word_list();
        let idx = (self.rand_u32() as usize) % words.len();
        let word = words[idx];
        let mut chars = [' '; WORD_LEN];
        for (i, ch) in word.chars().take(WORD_LEN).enumerate() {
            chars[i] = ch;
        }
        self.target = chars;
    }

    fn evaluate_guess(&self, guess: &[char; WORD_LEN]) -> [LetterResult; WORD_LEN] {
        let mut results = [LetterResult::Absent; WORD_LEN];
        let mut target_used = [false; WORD_LEN];

        // First pass: mark correct positions
        for i in 0..WORD_LEN {
            if guess[i] == self.target[i] {
                results[i] = LetterResult::Correct;
                target_used[i] = true;
            }
        }

        // Second pass: mark present letters
        for i in 0..WORD_LEN {
            if results[i] == LetterResult::Correct {
                continue;
            }
            for (j, used) in target_used.iter_mut().enumerate().take(WORD_LEN) {
                if !*used && guess[i] == self.target[j] {
                    results[i] = LetterResult::Present;
                    *used = true;
                    break;
                }
            }
        }

        results
    }

    fn submit_guess(&mut self) {
        if self.current_input.len() != WORD_LEN {
            self.message = Some("Not enough letters");
            return;
        }

        let mut word = [' '; WORD_LEN];
        for (i, &ch) in self.current_input.iter().enumerate() {
            word[i] = ch;
        }

        let guess_word: String = word.iter().collect();
        if !valid_guesses().contains(guess_word.as_str()) {
            self.message = Some("Invalid word (not in word list)");
            return;
        }

        let results = self.evaluate_guess(&word);
        let all_correct = results.iter().all(|r| *r == LetterResult::Correct);

        self.guesses.push(Guess { word, results });
        self.current_input.clear();
        self.message = None;

        if all_correct {
            self.won = true;
            self.game_over = true;
        } else if self.guesses.len() >= MAX_GUESSES {
            self.game_over = true;
        }
    }

    fn color_for_result(result: LetterResult) -> Color {
        match result {
            LetterResult::Correct => Color::Black,
            LetterResult::Present => Color::Black,
            LetterResult::Absent => Color::White,
        }
    }

    fn bg_for_result(result: LetterResult) -> Color {
        match result {
            LetterResult::Correct => Color::Green,
            LetterResult::Present => Color::Yellow,
            LetterResult::Absent => Color::DarkGray,
        }
    }
}

fn word_list() -> &'static Vec<&'static str> {
    WORD_LIST.get_or_init(|| {
        let mut words: Vec<&'static str> = include_str!("wordle_words.txt")
            .lines()
            .map(str::trim)
            .filter(|word| word.len() == WORD_LEN && word.chars().all(|ch| ch.is_ascii_lowercase()))
            .collect();
        words.sort_unstable();
        words.dedup();
        assert!(
            !words.is_empty(),
            "wordle word list must contain at least one word"
        );
        words
    })
}

fn valid_guesses() -> &'static HashSet<&'static str> {
    VALID_GUESSES.get_or_init(|| word_list().iter().copied().collect())
}

impl GameWidget for WordleGame {
    fn handle_key_event(&mut self, key_event: KeyEvent) -> bool {
        if key_event.kind != KeyEventKind::Press {
            return false;
        }

        if self.game_over {
            if matches!(key_event.code, KeyCode::Enter | KeyCode::Char(' ')) {
                self.reset();
                return true;
            }
            return false;
        }

        match key_event.code {
            KeyCode::Char(c) if c.is_ascii_alphabetic() => {
                if self.current_input.len() < WORD_LEN {
                    self.current_input.push(c.to_ascii_lowercase());
                    self.message = None;
                }
                true
            }
            KeyCode::Backspace => {
                self.current_input.pop();
                self.message = None;
                true
            }
            KeyCode::Enter => {
                self.submit_guess();
                true
            }
            _ => false,
        }
    }

    fn tick(&mut self) {
        // Wordle is turn-based, no tick needed
    }

    fn is_game_over(&self) -> bool {
        self.game_over
    }

    fn reset(&mut self) {
        self.guesses.clear();
        self.current_input.clear();
        self.won = false;
        self.game_over = false;
        self.message = None;
        self.pick_word();
    }

    fn render_game(&self, area: Rect, buf: &mut Buffer) {
        if area.height < 6 || area.width < 20 {
            return;
        }

        let dim = Style::default().fg(Color::DarkGray);
        let white = Style::default().fg(Color::White);

        // Each letter cell: " X " = 3 chars, plus border = 4 chars, plus trailing border
        let cell_w: u16 = 4;
        let board_w = cell_w * WORD_LEN as u16 + 1;
        let bx = area.x + area.width.saturating_sub(board_w) / 2;
        let mut y = area.y;
        let y_max = area.y + area.height;

        // Controls hint
        if y < y_max {
            buf.set_string(
                area.x + 1,
                y,
                "Type a 5-letter word, Enter to submit, Backspace to delete",
                dim,
            );
            y += 1;
        }

        // Spacer
        if y < y_max {
            y += 1;
        }

        // Render each guess row (completed guesses + current input + empty rows)
        for row in 0..MAX_GUESSES {
            if y >= y_max {
                break;
            }

            let mut x = bx;
            for col in 0..WORD_LEN {
                if x + cell_w > area.x + area.width {
                    break;
                }

                if row < self.guesses.len() {
                    // Completed guess
                    let guess = &self.guesses[row];
                    let bg = Self::bg_for_result(guess.results[col]);
                    let fg = Self::color_for_result(guess.results[col]);
                    let style = Style::default().fg(fg).bg(bg);
                    let letter = format!(" {} ", guess.word[col].to_ascii_uppercase());
                    buf.set_string(x, y, &letter, style);
                    buf.set_string(x + 3, y, " ", dim);
                } else if row == self.guesses.len() {
                    // Current input row
                    if col < self.current_input.len() {
                        let letter = format!(" {} ", self.current_input[col].to_ascii_uppercase());
                        buf.set_string(x, y, &letter, white);
                    } else {
                        buf.set_string(x, y, " _ ", dim);
                    }
                    buf.set_string(x + 3, y, " ", dim);
                } else {
                    // Empty future row
                    buf.set_string(x, y, " _ ", dim);
                    buf.set_string(x + 3, y, " ", dim);
                }

                x += cell_w;
            }
            y += 1;

            // Small spacer between rows
            if y < y_max && row < MAX_GUESSES - 1 {
                y += 1;
            }
        }

        // Spacer
        if y < y_max {
            y += 1;
        }

        // Status / message
        if y < y_max {
            if self.won {
                let turns = self.guesses.len();
                let msg = format!("You got it in {turns}! ");
                buf.set_string(area.x + 1, y, &msg, Style::default().fg(Color::Green));
                buf.set_string(
                    area.x + 1 + msg.len() as u16,
                    y,
                    "Enter for next word.",
                    dim,
                );
            } else if self.game_over {
                let answer: String = self.target.iter().map(char::to_ascii_uppercase).collect();
                let msg = format!("The word was {answer}. ");
                buf.set_string(area.x + 1, y, &msg, Style::default().fg(Color::Red));
                buf.set_string(
                    area.x + 1 + msg.len() as u16,
                    y,
                    "Enter for next word.",
                    dim,
                );
            } else if let Some(msg) = self.message {
                buf.set_string(area.x + 1, y, msg, Style::default().fg(Color::Yellow));
            } else {
                let remaining = MAX_GUESSES - self.guesses.len();
                let msg = format!(
                    "{} guess{} remaining",
                    remaining,
                    if remaining == 1 { "" } else { "es" }
                );
                buf.set_string(area.x + 1, y, &msg, dim);
            }
        }
    }

    fn game_desired_height(&self, _width: u16) -> u16 {
        // 1 controls + 1 spacer + 6 rows * 2 (row + spacer) - 1 + 1 spacer + 1 status = 15
        15
    }

    fn title(&self) -> &str {
        " Wordle "
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn rejects_guess_not_in_word_list() {
        let mut game = WordleGame::new();
        game.current_input = vec!['Z', 'Z', 'Z', 'Z', 'Z'];

        game.submit_guess();

        assert_eq!(game.guesses.len(), 0);
        assert_eq!(game.message, Some("Invalid word (not in word list)"));
    }

    #[test]
    fn word_list_is_non_empty_and_lowercase() {
        let words = word_list();

        assert_eq!(words.is_empty(), false);
        assert_eq!(words.iter().all(|word| word.len() == WORD_LEN), true);
        assert_eq!(
            words
                .iter()
                .all(|word| word.chars().all(|ch| ch.is_ascii_lowercase())),
            true
        );
    }
}
