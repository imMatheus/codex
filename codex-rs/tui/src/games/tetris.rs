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

const BOARD_W: usize = 10;
const BOARD_H: usize = 20;
const TICK_MS: u64 = 500;
const FAST_TICK_MS: u64 = 50;
const SPAWN_ROW: i32 = 3;
const SOFT_DROP_GRACE_MS: u64 = 175;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Cell {
    Empty,
    Filled(Color),
}

/// The 7 standard tetromino shapes, each with 4 rotations.
/// Each rotation is 4 (row, col) offsets from the piece origin.
type Rotation = [(i32, i32); 4];
type PieceRotations = [Rotation; 4];

const I_PIECE: PieceRotations = [
    [(0, 0), (0, 1), (0, 2), (0, 3)],
    [(0, 0), (1, 0), (2, 0), (3, 0)],
    [(0, 0), (0, 1), (0, 2), (0, 3)],
    [(0, 0), (1, 0), (2, 0), (3, 0)],
];
const O_PIECE: PieceRotations = [
    [(0, 0), (0, 1), (1, 0), (1, 1)],
    [(0, 0), (0, 1), (1, 0), (1, 1)],
    [(0, 0), (0, 1), (1, 0), (1, 1)],
    [(0, 0), (0, 1), (1, 0), (1, 1)],
];
const T_PIECE: PieceRotations = [
    [(0, 0), (0, 1), (0, 2), (1, 1)],
    [(0, 0), (1, 0), (2, 0), (1, 1)],
    [(1, 0), (1, 1), (1, 2), (0, 1)],
    [(0, 0), (1, 0), (2, 0), (1, -1)],
];
const S_PIECE: PieceRotations = [
    [(0, 1), (0, 2), (1, 0), (1, 1)],
    [(0, 0), (1, 0), (1, 1), (2, 1)],
    [(0, 1), (0, 2), (1, 0), (1, 1)],
    [(0, 0), (1, 0), (1, 1), (2, 1)],
];
const Z_PIECE: PieceRotations = [
    [(0, 0), (0, 1), (1, 1), (1, 2)],
    [(0, 1), (1, 0), (1, 1), (2, 0)],
    [(0, 0), (0, 1), (1, 1), (1, 2)],
    [(0, 1), (1, 0), (1, 1), (2, 0)],
];
const L_PIECE: PieceRotations = [
    [(0, 0), (0, 1), (0, 2), (1, 0)],
    [(0, 0), (1, 0), (2, 0), (2, 1)],
    [(1, 0), (1, 1), (1, 2), (0, 2)],
    [(0, 0), (0, 1), (1, 1), (2, 1)],
];
const J_PIECE: PieceRotations = [
    [(0, 0), (0, 1), (0, 2), (1, 2)],
    [(0, 0), (1, 0), (2, 0), (0, 1)],
    [(0, 0), (1, 0), (1, 1), (1, 2)],
    [(0, 1), (1, 1), (2, 0), (2, 1)],
];

const ALL_PIECES: [(&PieceRotations, Color); 7] = [
    (&I_PIECE, Color::Cyan),
    (&O_PIECE, Color::Yellow),
    (&T_PIECE, Color::Magenta),
    (&S_PIECE, Color::Green),
    (&Z_PIECE, Color::Red),
    (&L_PIECE, Color::LightYellow),
    (&J_PIECE, Color::Blue),
];

#[derive(Clone, Copy)]
struct ActivePiece {
    rotations: &'static PieceRotations,
    color: Color,
    rotation: usize,
    row: i32,
    col: i32,
}

impl ActivePiece {
    fn cells(&self) -> [(i32, i32); 4] {
        let offsets = self.rotations[self.rotation];
        let mut cells = [(0i32, 0i32); 4];
        for (i, (dr, dc)) in offsets.iter().enumerate() {
            cells[i] = (self.row + dr, self.col + dc);
        }
        cells
    }
}

pub(crate) struct TetrisGame {
    board: [[Cell; BOARD_W]; BOARD_H],
    piece: Option<ActivePiece>,
    score: u32,
    lines_cleared: u32,
    game_over: bool,
    last_tick: Instant,
    soft_drop: bool,
    last_soft_drop_input: Option<Instant>,
    frame_requester: FrameRequester,
    next_piece_index: usize,
}

impl TetrisGame {
    pub fn new(frame_requester: FrameRequester) -> Self {
        let mut game = Self {
            board: [[Cell::Empty; BOARD_W]; BOARD_H],
            piece: None,
            score: 0,
            lines_cleared: 0,
            game_over: false,
            last_tick: Instant::now(),
            soft_drop: false,
            last_soft_drop_input: None,
            frame_requester,
            next_piece_index: 0,
        };
        game.spawn_piece();
        game
    }

    fn spawn_piece(&mut self) {
        self.soft_drop = false;
        let (rotations, color) = ALL_PIECES[self.next_piece_index % ALL_PIECES.len()];
        self.next_piece_index += 1;
        let piece = ActivePiece {
            rotations,
            color,
            rotation: 0,
            row: SPAWN_ROW,
            col: (BOARD_W as i32) / 2 - 1,
        };
        // Check if spawn position is blocked
        if Self::collides_on_board(&self.board, &piece) {
            self.game_over = true;
            self.piece = None;
        } else {
            self.piece = Some(piece);
        }
    }

    fn collides_on_board(board: &[[Cell; BOARD_W]; BOARD_H], piece: &ActivePiece) -> bool {
        for (r, c) in piece.cells() {
            if c < 0 || c >= BOARD_W as i32 || r >= BOARD_H as i32 {
                return true;
            }
            if r >= 0 && board[r as usize][c as usize] != Cell::Empty {
                return true;
            }
        }
        false
    }

    fn lock_piece(&mut self) {
        if let Some(piece) = self.piece.take() {
            for (r, c) in piece.cells() {
                if r >= 0 && r < BOARD_H as i32 && c >= 0 && c < BOARD_W as i32 {
                    self.board[r as usize][c as usize] = Cell::Filled(piece.color);
                }
            }
            self.clear_lines();
            self.spawn_piece();
        }
    }

    fn clear_lines(&mut self) {
        let mut cleared = 0u32;
        let mut write = BOARD_H as i32 - 1;
        for read in (0..BOARD_H as i32).rev() {
            let read = read as usize;
            let full = self.board[read].iter().all(|c| *c != Cell::Empty);
            if full {
                cleared += 1;
            } else {
                if write as usize != read {
                    self.board[write as usize] = self.board[read];
                }
                write -= 1;
            }
        }
        while write >= 0 {
            self.board[write as usize] = [Cell::Empty; BOARD_W];
            write -= 1;
        }
        self.lines_cleared += cleared;
        self.score += match cleared {
            1 => 100,
            2 => 300,
            3 => 500,
            4 => 800,
            _ => 0,
        };
    }

    fn try_move(&mut self, dr: i32, dc: i32) -> bool {
        if let Some(piece) = &mut self.piece {
            piece.row += dr;
            piece.col += dc;
            if Self::collides_on_board(&self.board, piece) {
                piece.row -= dr;
                piece.col -= dc;
                return false;
            }
            true
        } else {
            false
        }
    }

    fn try_rotate(&mut self) {
        if let Some(piece) = &mut self.piece {
            let old_rot = piece.rotation;
            piece.rotation = (piece.rotation + 1) % 4;
            if Self::collides_on_board(&self.board, piece) {
                // Try wall kicks: left, right
                piece.col -= 1;
                if !Self::collides_on_board(&self.board, piece) {
                    return;
                }
                piece.col += 2;
                if !Self::collides_on_board(&self.board, piece) {
                    return;
                }
                // Revert
                piece.col -= 1;
                piece.rotation = old_rot;
            }
        }
    }

    fn drop_tick(&mut self) {
        if !self.try_move(1, 0) {
            self.lock_piece();
        }
    }
}

impl GameWidget for TetrisGame {
    fn handle_key_event(&mut self, key_event: KeyEvent) -> bool {
        if key_event.kind == KeyEventKind::Release {
            if key_event.code == KeyCode::Down {
                self.soft_drop = false;
                self.last_soft_drop_input = None;
                return true;
            }
            return false;
        }
        if !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
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
            KeyCode::Left => {
                self.try_move(0, -1);
                true
            }
            KeyCode::Right => {
                self.try_move(0, 1);
                true
            }
            KeyCode::Up | KeyCode::Char(' ') => {
                self.try_rotate();
                true
            }
            KeyCode::Down => {
                self.soft_drop = true;
                self.last_soft_drop_input = Some(Instant::now());
                self.drop_tick();
                self.last_tick = Instant::now();
                true
            }
            KeyCode::Enter => {
                // Hard drop
                while self.try_move(1, 0) {}
                self.lock_piece();
                true
            }
            _ => false,
        }
    }

    fn tick(&mut self) {
        if self.game_over {
            return;
        }
        let now = Instant::now();
        if self.soft_drop
            && self.last_soft_drop_input.is_some_and(|last| {
                now.duration_since(last) > Duration::from_millis(SOFT_DROP_GRACE_MS)
            })
        {
            self.soft_drop = false;
            self.last_soft_drop_input = None;
        }
        let interval = if self.soft_drop {
            Duration::from_millis(FAST_TICK_MS)
        } else {
            Duration::from_millis(TICK_MS)
        };
        if now.duration_since(self.last_tick) >= interval {
            self.last_tick = now;
            self.drop_tick();
        }
        self.frame_requester
            .schedule_frame_in(Duration::from_millis(FAST_TICK_MS));
    }

    fn is_game_over(&self) -> bool {
        self.game_over
    }

    fn reset(&mut self) {
        self.board = [[Cell::Empty; BOARD_W]; BOARD_H];
        self.piece = None;
        self.score = 0;
        self.lines_cleared = 0;
        self.game_over = false;
        self.last_tick = Instant::now();
        self.soft_drop = false;
        self.last_soft_drop_input = None;
        self.next_piece_index = 0;
        self.spawn_piece();
    }

    fn render_game(&self, area: Rect, buf: &mut Buffer) {
        if area.height < 6 || area.width < 14 {
            return;
        }

        let dim = Style::default().fg(Color::DarkGray);
        let white = Style::default().fg(Color::White);

        // Board rendering: each cell is 2 chars wide "[]"
        // So board is 2 * BOARD_W + 2 (for left/right walls) = 22
        let cell_w: u16 = 2;
        let board_render_w = cell_w * BOARD_W as u16 + 2;
        let bx = area.x + area.width.saturating_sub(board_render_w + 14) / 2;
        let mut y = area.y;
        let y_max = area.y + area.height;

        // Controls hint
        if y < y_max {
            buf.set_string(
                area.x + 1,
                y,
                "Arrows: move  Up/Space: rotate  Enter: drop",
                dim,
            );
            y += 1;
        }

        // Spacer
        if y < y_max {
            y += 1;
        }

        // Collect active piece cells for rendering
        let piece_cells: [(i32, i32); 4] = self
            .piece
            .as_ref()
            .map(ActivePiece::cells)
            .unwrap_or([(-1, -1); 4]);
        let piece_color = self.piece.as_ref().map(|p| p.color).unwrap_or(Color::White);
        let ghost_cells: [(i32, i32); 4] = self
            .piece
            .as_ref()
            .map(|piece| {
                let mut ghost = *piece;
                loop {
                    let mut dropped = ghost;
                    dropped.row += 1;
                    if Self::collides_on_board(&self.board, &dropped) {
                        break ghost.cells();
                    }
                    ghost = dropped;
                }
            })
            .unwrap_or([(-1, -1); 4]);

        // Keep the board anchored at the bottom so placed blocks stay visible
        // when a new piece spawns near the top in compact layouts.
        let visible_rows = y_max.saturating_sub(y).saturating_sub(1) as usize;
        let visible_rows = visible_rows.min(BOARD_H);
        let visible_start = BOARD_H.saturating_sub(visible_rows);
        let visible_end = (visible_start + visible_rows).min(BOARD_H);

        // Board rows
        for row in visible_start..visible_end {
            if y >= y_max {
                break;
            }
            let mut x = bx;
            // Left wall
            buf.set_string(x, y, "\u{2502}", dim);
            x += 1;

            for col in 0..BOARD_W {
                let is_piece = piece_cells
                    .iter()
                    .any(|&(pr, pc)| pr == row as i32 && pc == col as i32);
                let is_ghost = ghost_cells
                    .iter()
                    .any(|&(gr, gc)| gr == row as i32 && gc == col as i32);

                if is_piece {
                    buf.set_string(x, y, "\u{2588}\u{2588}", Style::default().fg(piece_color));
                } else {
                    match self.board[row][col] {
                        Cell::Filled(color) => {
                            buf.set_string(x, y, "\u{2588}\u{2588}", Style::default().fg(color));
                        }
                        Cell::Empty => {
                            if is_ghost {
                                buf.set_string(
                                    x,
                                    y,
                                    "\u{2591}\u{2591}",
                                    Style::default().fg(piece_color),
                                );
                            } else {
                                buf.set_string(x, y, "\u{00b7} ", dim);
                            }
                        }
                    }
                }
                x += cell_w;
            }
            // Right wall
            buf.set_string(x, y, "\u{2502}", dim);
            y += 1;
        }

        // Bottom wall
        if y < y_max {
            let mut bottom = String::with_capacity(board_render_w as usize);
            bottom.push('\u{2514}');
            for _ in 0..BOARD_W * 2 {
                bottom.push('\u{2500}');
            }
            bottom.push('\u{2518}');
            buf.set_string(bx, y, &bottom, dim);
            y += 1;
        }

        // Score / lines on the right side
        let info_x = bx + board_render_w + 2;
        let info_y = area.y + 2;
        if info_x + 10 < area.x + area.width {
            buf.set_string(info_x, info_y, "Score", dim);
            buf.set_string(info_x, info_y + 1, self.score.to_string(), white);
            buf.set_string(info_x, info_y + 3, "Lines", dim);
            buf.set_string(info_x, info_y + 4, self.lines_cleared.to_string(), white);

            let next_y = info_y + 6;
            if next_y + 6 < y_max {
                buf.set_string(info_x, next_y, "Next", dim);
                buf.set_string(
                    info_x,
                    next_y + 1,
                    "\u{250c}\u{2500}\u{2500}\u{2500}\u{2500}\u{2510}",
                    dim,
                );
                for row in 0..4 {
                    buf.set_string(info_x, next_y + 2 + row, "\u{2502}    \u{2502}", dim);
                }
                buf.set_string(
                    info_x,
                    next_y + 6,
                    "\u{2514}\u{2500}\u{2500}\u{2500}\u{2500}\u{2518}",
                    dim,
                );

                let (next_rotations, next_color) =
                    ALL_PIECES[self.next_piece_index % ALL_PIECES.len()];
                let offsets = next_rotations[0];
                let mut min_r = offsets[0].0;
                let mut max_r = offsets[0].0;
                let mut min_c = offsets[0].1;
                let mut max_c = offsets[0].1;
                for (r, c) in offsets.iter().skip(1) {
                    min_r = min_r.min(*r);
                    max_r = max_r.max(*r);
                    min_c = min_c.min(*c);
                    max_c = max_c.max(*c);
                }
                let piece_h = (max_r - min_r + 1) as u16;
                let piece_w = (max_c - min_c + 1) as u16;
                let offset_r = (4u16.saturating_sub(piece_h)) / 2;
                let offset_c = (4u16.saturating_sub(piece_w)) / 2;
                for (r, c) in offsets {
                    let rr = (r - min_r) as u16 + offset_r;
                    let cc = (c - min_c) as u16 + offset_c;
                    buf.set_string(
                        info_x + 1 + cc,
                        next_y + 2 + rr,
                        "\u{2588}",
                        Style::default().fg(next_color),
                    );
                }
            }
        }

        // Game over message
        if self.game_over && y < y_max {
            y += 1;
            if y < y_max {
                buf.set_string(
                    area.x + 1,
                    y,
                    "Game Over! ",
                    Style::default().fg(Color::Red),
                );
                buf.set_string(area.x + 12, y, "Press Enter to restart.", dim);
            }
        }
    }

    fn game_desired_height(&self, _width: u16) -> u16 {
        // 1 controls + 1 spacer + 16 visible rows + 1 bottom wall + 1 spacer = 20
        20
    }

    fn title(&self) -> &str {
        " Tetris "
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    use super::*;

    #[test]
    fn compact_view_keeps_bottom_locked_cells_visible_after_spawn() {
        let mut game = TetrisGame::new(FrameRequester::test_dummy());
        game.board[BOARD_H - 1][0] = Cell::Filled(Color::Red);
        let area = Rect::new(0, 0, 40, 20);
        let mut buf = Buffer::empty(area);

        game.render_game(area, &mut buf);

        let cell_w: u16 = 2;
        let board_render_w = cell_w * BOARD_W as u16 + 2;
        let bx = area.x + area.width.saturating_sub(board_render_w + 14) / 2;
        let board_top_y = area.y + 2;
        let visible_rows = (area.y + area.height)
            .saturating_sub(board_top_y)
            .saturating_sub(1) as usize;
        let visible_rows = visible_rows.min(BOARD_H);
        let visible_start = BOARD_H.saturating_sub(visible_rows);
        let bottom_row_y = board_top_y + (BOARD_H - 1 - visible_start) as u16;

        assert_eq!(buf[(bx + 1, bottom_row_y)].symbol(), "\u{2588}");
    }

    #[test]
    fn spawned_piece_is_immediately_visible_in_compact_view() {
        let game = TetrisGame::new(FrameRequester::test_dummy());
        let area = Rect::new(0, 0, 40, 20);
        let mut buf = Buffer::empty(area);

        game.render_game(area, &mut buf);

        let mut has_active_piece_block = false;
        for yy in area.y..area.y + area.height {
            for xx in area.x..area.x + area.width {
                if buf[(xx, yy)].symbol() == "\u{2588}" {
                    has_active_piece_block = true;
                    break;
                }
            }
            if has_active_piece_block {
                break;
            }
        }

        assert_eq!(has_active_piece_block, true);
    }

    #[test]
    fn renders_ghost_piece_outline() {
        let game = TetrisGame::new(FrameRequester::test_dummy());
        let area = Rect::new(0, 0, 60, 24);
        let mut buf = Buffer::empty(area);

        game.render_game(area, &mut buf);

        let mut has_ghost_block = false;
        for yy in area.y..area.y + area.height {
            for xx in area.x..area.x + area.width {
                if buf[(xx, yy)].symbol() == "\u{2591}" {
                    has_ghost_block = true;
                    break;
                }
            }
            if has_ghost_block {
                break;
            }
        }

        assert_eq!(has_ghost_block, true);
    }

    #[test]
    fn renders_next_piece_preview_box() {
        let game = TetrisGame::new(FrameRequester::test_dummy());
        let area = Rect::new(0, 0, 60, 24);
        let mut buf = Buffer::empty(area);

        game.render_game(area, &mut buf);

        let cell_w: u16 = 2;
        let board_render_w = cell_w * BOARD_W as u16 + 2;
        let bx = area.x + area.width.saturating_sub(board_render_w + 14) / 2;
        let info_x = bx + board_render_w + 2;
        let next_y = area.y + 2 + 6;
        let label: String = (0..4)
            .map(|dx| buf[(info_x + dx, next_y)].symbol())
            .collect();

        let mut has_preview_block = false;
        for yy in (next_y + 2)..=(next_y + 5) {
            for xx in (info_x + 1)..=(info_x + 4) {
                if buf[(xx, yy)].symbol() == "\u{2588}" {
                    has_preview_block = true;
                    break;
                }
            }
            if has_preview_block {
                break;
            }
        }

        assert_eq!(label, "Next");
        assert_eq!(has_preview_block, true);
    }
}
