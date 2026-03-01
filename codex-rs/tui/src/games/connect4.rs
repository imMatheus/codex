use std::time::Duration;
use std::time::Instant;

use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;

use super::GameWidget;
use crate::tui::FrameRequester;

const COLS: usize = 7;
const ROWS: usize = 6;
const AI_DELAY: Duration = Duration::from_millis(500);
const SEARCH_DEPTH: u32 = 10;
const MOVE_ORDER: [usize; COLS] = [3, 2, 4, 1, 5, 0, 6];
const WIN_SCORE: i32 = 100_000;

type Board = [[Option<Player>; COLS]; ROWS];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Player {
    Human,
    Ai,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GameResult {
    Win(Player),
    Draw,
}

pub(crate) struct Connect4Game {
    board: [[Option<Player>; COLS]; ROWS],
    selected_col: usize,
    current_player: Player,
    result: Option<GameResult>,
    move_count: u32,
    /// The cells that form the winning line (if any).
    winning_cells: Vec<(usize, usize)>,
    /// When set, the AI will place its piece after this instant.
    ai_pending_at: Option<Instant>,
    frame_requester: FrameRequester,
}

impl Connect4Game {
    pub fn new(frame_requester: FrameRequester) -> Self {
        Self {
            board: [[None; COLS]; ROWS],
            selected_col: 3, // center
            current_player: Player::Human,
            result: None,
            move_count: 0,
            ai_pending_at: None,
            frame_requester,
            winning_cells: Vec::new(),
        }
    }

    fn drop_piece(&mut self, col: usize) -> Option<(usize, usize)> {
        // Find the lowest empty row in this column
        for row in (0..ROWS).rev() {
            if self.board[row][col].is_none() {
                self.board[row][col] = Some(self.current_player);
                self.move_count += 1;
                return Some((row, col));
            }
        }
        None
    }

    fn column_full(&self, col: usize) -> bool {
        self.board[0][col].is_some()
    }

    fn check_win(&self, row: usize, col: usize) -> Option<Vec<(usize, usize)>> {
        let player = self.board[row][col]?;

        let directions: [(i32, i32); 4] = [(0, 1), (1, 0), (1, 1), (1, -1)];

        for (dr, dc) in &directions {
            let mut cells = vec![(row, col)];
            // Check in the positive direction
            for i in 1..4 {
                let r = row as i32 + dr * i;
                let c = col as i32 + dc * i;
                if r >= 0
                    && r < ROWS as i32
                    && c >= 0
                    && c < COLS as i32
                    && self.board[r as usize][c as usize] == Some(player)
                {
                    cells.push((r as usize, c as usize));
                } else {
                    break;
                }
            }
            // Check in the negative direction
            for i in 1..4 {
                let r = row as i32 - dr * i;
                let c = col as i32 - dc * i;
                if r >= 0
                    && r < ROWS as i32
                    && c >= 0
                    && c < COLS as i32
                    && self.board[r as usize][c as usize] == Some(player)
                {
                    cells.push((r as usize, c as usize));
                } else {
                    break;
                }
            }
            if cells.len() >= 4 {
                return Some(cells);
            }
        }
        None
    }

    fn is_draw(&self) -> bool {
        self.move_count as usize >= ROWS * COLS
    }

    fn ai_move(&mut self) {
        if let Some(col) = self.find_best_move() {
            self.make_move(col);
        }
    }

    fn find_best_move(&self) -> Option<usize> {
        let mut best_score = i32::MIN + 1;
        let mut best_col = None;
        let mut alpha = i32::MIN + 1;
        let beta = i32::MAX;

        for &col in &MOVE_ORDER {
            if self.column_full(col) {
                continue;
            }
            let mut board = self.board;
            if let Some(row) = drop_on_board(&mut board, col, Player::Ai) {
                let score = if has_four_at(&board, row, col) {
                    WIN_SCORE + SEARCH_DEPTH as i32
                } else {
                    minimax(
                        &board,
                        SEARCH_DEPTH - 1,
                        alpha,
                        beta,
                        false,
                        self.move_count + 1,
                    )
                };
                if score > best_score {
                    best_score = score;
                    best_col = Some(col);
                }
                alpha = alpha.max(score);
            }
        }
        best_col
    }

    fn make_move(&mut self, col: usize) {
        if let Some((row, col)) = self.drop_piece(col) {
            if let Some(cells) = self.check_win(row, col) {
                self.winning_cells = cells;
                self.result = Some(GameResult::Win(self.current_player));
            } else if self.is_draw() {
                self.result = Some(GameResult::Draw);
            } else {
                self.current_player = match self.current_player {
                    Player::Human => Player::Ai,
                    Player::Ai => Player::Human,
                };
            }
        }
    }
}

// ── Minimax AI search ──

fn drop_on_board(board: &mut Board, col: usize, player: Player) -> Option<usize> {
    for row in (0..ROWS).rev() {
        if board[row][col].is_none() {
            board[row][col] = Some(player);
            return Some(row);
        }
    }
    None
}

fn has_four_at(board: &Board, row: usize, col: usize) -> bool {
    let player = match board[row][col] {
        Some(p) => p,
        None => return false,
    };
    let dirs: [(i32, i32); 4] = [(0, 1), (1, 0), (1, 1), (1, -1)];
    for &(dr, dc) in &dirs {
        let mut count = 1u32;
        for &sign in &[1i32, -1] {
            for i in 1..4i32 {
                let r = row as i32 + dr * i * sign;
                let c = col as i32 + dc * i * sign;
                if r >= 0
                    && r < ROWS as i32
                    && c >= 0
                    && c < COLS as i32
                    && board[r as usize][c as usize] == Some(player)
                {
                    count += 1;
                } else {
                    break;
                }
            }
        }
        if count >= 4 {
            return true;
        }
    }
    false
}

fn minimax(
    board: &Board,
    depth: u32,
    mut alpha: i32,
    mut beta: i32,
    maximizing: bool,
    moves: u32,
) -> i32 {
    if moves >= (ROWS * COLS) as u32 {
        return 0;
    }
    if depth == 0 {
        return evaluate(board);
    }

    let player = if maximizing {
        Player::Ai
    } else {
        Player::Human
    };
    let mut best = if maximizing { i32::MIN + 1 } else { i32::MAX };
    let mut found_move = false;

    for &col in &MOVE_ORDER {
        if board[0][col].is_some() {
            continue;
        }
        found_move = true;
        let mut b = *board;
        if let Some(row) = drop_on_board(&mut b, col, player) {
            if has_four_at(&b, row, col) {
                let win = WIN_SCORE + depth as i32;
                return if maximizing { win } else { -win };
            }
            let score = minimax(&b, depth - 1, alpha, beta, !maximizing, moves + 1);
            if maximizing {
                best = best.max(score);
                alpha = alpha.max(best);
            } else {
                best = best.min(score);
                beta = beta.min(best);
            }
            if alpha >= beta {
                break;
            }
        }
    }

    if !found_move { 0 } else { best }
}

fn evaluate(board: &Board) -> i32 {
    let mut score: i32 = 0;

    // Center column preference
    let center = COLS / 2;
    for row in board {
        match row[center] {
            Some(Player::Ai) => score += 3,
            Some(Player::Human) => score -= 3,
            None => {}
        }
    }

    // Horizontal windows
    for row in board {
        for c in 0..=COLS - 4 {
            score += score_window(row[c], row[c + 1], row[c + 2], row[c + 3]);
        }
    }

    // Vertical windows
    for r in 0..=ROWS - 4 {
        for c in 0..COLS {
            score += score_window(
                board[r][c],
                board[r + 1][c],
                board[r + 2][c],
                board[r + 3][c],
            );
        }
    }

    // Diagonal down-right
    for r in 0..=ROWS - 4 {
        for c in 0..=COLS - 4 {
            score += score_window(
                board[r][c],
                board[r + 1][c + 1],
                board[r + 2][c + 2],
                board[r + 3][c + 3],
            );
        }
    }

    // Diagonal up-right
    for r in 3..ROWS {
        for c in 0..=COLS - 4 {
            score += score_window(
                board[r][c],
                board[r - 1][c + 1],
                board[r - 2][c + 2],
                board[r - 3][c + 3],
            );
        }
    }

    score
}

fn score_window(a: Option<Player>, b: Option<Player>, c: Option<Player>, d: Option<Player>) -> i32 {
    let cells = [a, b, c, d];
    let ai = cells.iter().filter(|p| **p == Some(Player::Ai)).count();
    let human = cells.iter().filter(|p| **p == Some(Player::Human)).count();

    // Mixed windows (both players present) have no strategic value.
    if ai > 0 && human > 0 {
        return 0;
    }

    (match ai {
        3 => 5,
        2 => 2,
        _ => 0,
    }) - match human {
        3 => 5,
        2 => 2,
        _ => 0,
    }
}

impl GameWidget for Connect4Game {
    fn handle_key_event(&mut self, key_event: KeyEvent) -> bool {
        if key_event.kind != KeyEventKind::Press {
            return false;
        }

        if self.result.is_some() {
            // Game over — Enter restarts
            if matches!(key_event.code, KeyCode::Enter | KeyCode::Char(' ')) {
                self.reset();
                return true;
            }
            return false;
        }

        if self.current_player != Player::Human {
            return false;
        }

        match key_event.code {
            KeyCode::Left => {
                if self.selected_col > 0 {
                    self.selected_col -= 1;
                }
                true
            }
            KeyCode::Right => {
                if self.selected_col < COLS - 1 {
                    self.selected_col += 1;
                }
                true
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if !self.column_full(self.selected_col) {
                    self.make_move(self.selected_col);
                    // Schedule the AI move after a short delay
                    if self.result.is_none() && self.current_player == Player::Ai {
                        self.ai_pending_at = Some(Instant::now() + AI_DELAY);
                        self.frame_requester.schedule_frame_in(AI_DELAY);
                    }
                }
                true
            }
            _ => false,
        }
    }

    fn tick(&mut self) {
        if let Some(deadline) = self.ai_pending_at
            && Instant::now() >= deadline
        {
            self.ai_pending_at = None;
            self.ai_move();
        }
    }

    fn is_game_over(&self) -> bool {
        self.result.is_some()
    }

    fn reset(&mut self) {
        self.board = [[None; COLS]; ROWS];
        self.selected_col = 3;
        self.current_player = Player::Human;
        self.result = None;
        self.move_count = 0;
        self.ai_pending_at = None;
        self.winning_cells.clear();
    }

    fn render_game(&self, area: Rect, buf: &mut Buffer) {
        if area.height < 4 || area.width < 20 {
            return;
        }

        // Each cell is 4 chars wide: "| \u{25cb} " with a trailing "|" at the end.
        // Total board width: 4 * COLS + 1 = 29
        let cell_w: u16 = 4;
        let board_width = cell_w * COLS as u16 + 1;
        let bx = area.x + area.width.saturating_sub(board_width) / 2;
        let dim = Style::default().fg(Color::DarkGray);
        let board = Style::default().fg(Color::Blue);
        let bold_white = Style::default().fg(Color::White);
        let cursor = Style::default().fg(Color::Cyan);
        let human_disc = Style::default().fg(Color::LightRed);
        let ai_disc = Style::default().fg(Color::LightYellow);
        let human_winning_disc = Style::default().fg(Color::White).bg(Color::LightRed);
        let ai_winning_disc = Style::default().fg(Color::Black).bg(Color::LightYellow);

        let mut y = area.y;
        let y_max = area.y + area.height;

        // Row 0: Controls hint
        if y < y_max {
            let hint = "Controls: Left/Right to move, Enter/Space to drop";
            buf.set_string(area.x + 1, y, hint, dim);
            y += 1;
        }

        // Row 1: blank spacer
        if y < y_max {
            y += 1;
        }

        // Row 2: Column selector "v" above the selected column
        if y < y_max && self.result.is_none() {
            // The center of column `c` is at bx + 2 + c * cell_w
            let cursor_x = bx + 2 + self.selected_col as u16 * cell_w;
            buf.set_string(cursor_x, y, "v", cursor);
        }
        y += 1;

        // Row 3: Top border "+---+---+---+---+---+---+---+"
        if y < y_max {
            let mut border = String::with_capacity(board_width as usize);
            for _ in 0..COLS {
                border.push('+');
                border.push_str("---");
            }
            border.push('+');
            buf.set_string(bx, y, &border, board);
            y += 1;
        }

        // Rows 4-9: Board rows "| \u{25cb} | \u{25cb} | ... |"
        for row in 0..ROWS {
            if y >= y_max {
                break;
            }
            let mut x = bx;
            for col in 0..COLS {
                if x + cell_w >= area.x + area.width {
                    break;
                }
                buf.set_string(x, y, "| ", dim);
                x += 2;

                let is_winning = self
                    .winning_cells
                    .iter()
                    .any(|&(wr, wc)| wr == row && wc == col);

                match self.board[row][col] {
                    Some(Player::Human) => {
                        let style = if is_winning {
                            human_winning_disc
                        } else {
                            human_disc
                        };
                        let ch = if is_winning { "\u{25c9}" } else { "\u{25cf}" };
                        buf.set_string(x, y, ch, style);
                    }
                    Some(Player::Ai) => {
                        let style = if is_winning { ai_winning_disc } else { ai_disc };
                        let ch = if is_winning { "\u{25c9}" } else { "\u{25cf}" };
                        buf.set_string(x, y, ch, style);
                    }
                    None => {
                        buf.set_string(x, y, "\u{25cb}", dim);
                    }
                }
                x += 1;
                buf.set_string(x, y, " ", dim);
                x += 1;
            }
            // Trailing pipe
            if x < area.x + area.width {
                buf.set_string(x, y, "|", dim);
            }
            y += 1;
        }

        // Bottom border
        if y < y_max {
            let mut border = String::with_capacity(board_width as usize);
            for _ in 0..COLS {
                border.push('+');
                border.push_str("---");
            }
            border.push('+');
            buf.set_string(bx, y, &border, board);
            y += 1;
        }

        // Column numbers "  1   2   3   4   5   6   7"
        if y < y_max {
            for col in 0..COLS {
                let num_x = bx + 2 + col as u16 * cell_w;
                let num_str = format!("{}", col + 1);
                let style = if col == self.selected_col && self.result.is_none() {
                    cursor
                } else {
                    dim
                };
                buf.set_string(num_x, y, &num_str, style);
            }
            y += 1;
        }

        // Blank spacer
        if y < y_max {
            y += 1;
        }

        // Status line: "Your turn (Red \u{25cf})" / "AI thinking..." / result
        if y < y_max {
            match self.result {
                Some(GameResult::Win(Player::Human)) => {
                    buf.set_string(area.x + 1, y, "You win! ", human_disc);
                    buf.set_string(area.x + 10, y, "Press Enter to play again.", dim);
                }
                Some(GameResult::Win(Player::Ai)) => {
                    buf.set_string(area.x + 1, y, "AI wins! ", ai_disc);
                    buf.set_string(area.x + 10, y, "Press Enter to play again.", dim);
                }
                Some(GameResult::Draw) => {
                    buf.set_string(area.x + 1, y, "Draw! ", Style::default().fg(Color::White));
                    buf.set_string(area.x + 7, y, "Press Enter to play again.", dim);
                }
                None => {
                    // "Your turn (Red \u{25cf})"
                    buf.set_string(area.x + 1, y, "Your turn ", bold_white);
                    buf.set_string(area.x + 11, y, "(", dim);
                    buf.set_string(area.x + 12, y, "Red", human_disc);
                    buf.set_string(area.x + 15, y, " ", dim);
                    buf.set_string(area.x + 16, y, "\u{25cf}", human_disc);
                    buf.set_string(area.x + 17, y, ")", dim);
                }
            }
        }
    }

    fn game_desired_height(&self, _width: u16) -> u16 {
        // 1 controls + 1 spacer + 1 cursor + 1 top border + 6 rows + 1 bottom border
        // + 1 column numbers + 1 spacer + 1 status = 14
        14
    }

    fn title(&self) -> &str {
        " Connect 4 "
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    use super::*;

    #[test]
    fn winning_human_piece_uses_high_contrast_highlight() {
        let mut game = Connect4Game::new(FrameRequester::test_dummy());
        game.board[ROWS - 1][0] = Some(Player::Human);
        game.winning_cells = vec![(ROWS - 1, 0)];

        let area = Rect::new(0, 0, 40, 20);
        let mut buf = Buffer::empty(area);
        game.render_game(area, &mut buf);

        let cell_w: u16 = 4;
        let board_width = cell_w * COLS as u16 + 1;
        let bx = area.x + area.width.saturating_sub(board_width) / 2;
        let chip_x = bx + 2;
        let chip_y = area.y + 4 + (ROWS - 1) as u16;
        let chip = &buf[(chip_x, chip_y)];

        assert_eq!(chip.symbol(), "\u{25c9}");
        assert_eq!(chip.style().fg, Some(Color::White));
        assert_eq!(chip.style().bg, Some(Color::LightRed));
    }
}
