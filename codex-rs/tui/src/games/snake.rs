use std::collections::VecDeque;
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

const TICK_MS: u64 = 100;
const INITIAL_LEN: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    fn opposite(self) -> Self {
        match self {
            Dir::Up => Dir::Down,
            Dir::Down => Dir::Up,
            Dir::Left => Dir::Right,
            Dir::Right => Dir::Left,
        }
    }

    fn delta(self) -> (i32, i32) {
        match self {
            Dir::Up => (-1, 0),
            Dir::Down => (1, 0),
            Dir::Left => (0, -1),
            Dir::Right => (0, 1),
        }
    }
}

pub(crate) struct SnakeGame {
    body: VecDeque<(i32, i32)>,
    dir: Dir,
    queued_dir: Option<Dir>,
    food: (i32, i32),
    score: u32,
    game_over: bool,
    field_w: i32,
    field_h: i32,
    frame_requester: FrameRequester,
    last_tick: Instant,
    spawn_counter: u32,
}

impl SnakeGame {
    pub fn new(frame_requester: FrameRequester) -> Self {
        let mut game = Self {
            body: VecDeque::new(),
            dir: Dir::Right,
            queued_dir: None,
            food: (0, 0),
            score: 0,
            game_over: false,
            field_w: 30,
            field_h: 15,
            frame_requester,
            last_tick: Instant::now(),
            spawn_counter: 7,
        };
        game.init_snake();
        game.place_food();
        game
    }

    fn init_snake(&mut self) {
        self.body.clear();
        let start_r = self.field_h / 2;
        let start_c = self.field_w / 4;
        for i in 0..INITIAL_LEN as i32 {
            // Front is the tail and back is the head.
            self.body.push_back((start_r, start_c + i));
        }
    }

    fn pseudo_rand(&mut self) -> u32 {
        self.spawn_counter = self
            .spawn_counter
            .wrapping_mul(1103515245)
            .wrapping_add(12345);
        (self.spawn_counter >> 16) & 0x7FFF
    }

    fn place_food(&mut self) {
        for _ in 0..200 {
            let r = (self.pseudo_rand() as i32) % self.field_h;
            let c = (self.pseudo_rand() as i32) % self.field_w;
            if !self.body.iter().any(|&(br, bc)| br == r && bc == c) {
                self.food = (r, c);
                return;
            }
        }
        // Fallback: just pick something
        self.food = (0, 0);
    }

    fn step(&mut self) {
        if let Some(d) = self.queued_dir.take() {
            self.dir = d;
        }

        let Some(&(hr, hc)) = self.body.back() else {
            self.game_over = true;
            return;
        };
        let (dr, dc) = self.dir.delta();
        let nr = hr + dr;
        let nc = hc + dc;

        // Wall collision
        if nr < 0 || nr >= self.field_h || nc < 0 || nc >= self.field_w {
            self.game_over = true;
            return;
        }

        // Self collision (skip tail since it will move unless we grow)
        let will_grow = nr == self.food.0 && nc == self.food.1;
        for (i, &(br, bc)) in self.body.iter().enumerate() {
            if br == nr && bc == nc {
                // If it's the very tail and we won't grow, it will move out
                if i == 0 && !will_grow {
                    continue;
                }
                self.game_over = true;
                return;
            }
        }

        self.body.push_back((nr, nc));

        if will_grow {
            self.score += 10;
            self.place_food();
        } else {
            self.body.pop_front();
        }
    }
}

impl GameWidget for SnakeGame {
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

        let new_dir = match key_event.code {
            KeyCode::Up => Some(Dir::Up),
            KeyCode::Down => Some(Dir::Down),
            KeyCode::Left => Some(Dir::Left),
            KeyCode::Right => Some(Dir::Right),
            _ => None,
        };

        if let Some(d) = new_dir {
            let current = self.queued_dir.unwrap_or(self.dir);
            if d != current.opposite() {
                self.queued_dir = Some(d);
            }
            return true;
        }

        false
    }

    fn tick(&mut self) {
        if self.game_over {
            return;
        }
        let now = Instant::now();
        if now.duration_since(self.last_tick) < Duration::from_millis(TICK_MS) {
            self.frame_requester
                .schedule_frame_in(Duration::from_millis(TICK_MS));
            return;
        }
        self.last_tick = now;
        self.step();
        self.frame_requester
            .schedule_frame_in(Duration::from_millis(TICK_MS));
    }

    fn is_game_over(&self) -> bool {
        self.game_over
    }

    fn reset(&mut self) {
        self.dir = Dir::Right;
        self.queued_dir = None;
        self.score = 0;
        self.game_over = false;
        self.last_tick = Instant::now();
        self.init_snake();
        self.place_food();
    }

    fn render_game(&self, area: Rect, buf: &mut Buffer) {
        if area.height < 6 || area.width < 20 {
            return;
        }

        let dim = Style::default().fg(Color::DarkGray);
        let border_style = Style::default().fg(Color::Gray);
        let white = Style::default().fg(Color::White);
        let head_style = Style::default().fg(Color::LightGreen);
        let body_style = Style::default().fg(Color::Green);
        let food_style = Style::default().fg(Color::Red);
        let score_label = Style::default().fg(Color::Gray);

        // Each cell is 2 chars wide so the field looks square-ish
        let cell_w: u16 = 2;
        // Adapt field dimensions to available space
        let max_field_w = ((area.width.saturating_sub(4)) / cell_w) as i32;
        let max_field_h = (area.height.saturating_sub(4)) as i32;
        let fw = max_field_w.min(self.field_w);
        let fh = max_field_h.min(self.field_h);
        let board_w = fw as u16 * cell_w + 2; // +2 for left/right border
        let bx = area.x + area.width.saturating_sub(board_w) / 2;

        let mut y = area.y;
        let y_max = area.y + area.height;

        // Header
        if y < y_max {
            buf.set_string(area.x + 1, y, "Arrow keys to move", dim);
            let score_text = format!("{}", self.score);
            let label = "Score ";
            let sx = area.x
                + area
                    .width
                    .saturating_sub((label.len() + score_text.len()) as u16 + 1);
            buf.set_string(sx, y, label, score_label);
            buf.set_string(sx + label.len() as u16, y, &score_text, white);
            y += 1;
        }

        // Top border
        if y < y_max {
            let mut border = String::new();
            border.push('\u{250c}'); // ┌
            for _ in 0..fw as u16 * cell_w {
                border.push('\u{2500}'); // ─
            }
            border.push('\u{2510}'); // ┐
            buf.set_string(bx, y, &border, border_style);
            y += 1;
        }

        // Field rows
        let head = self.body.back().copied();
        for row in 0..fh {
            if y >= y_max {
                break;
            }
            buf.set_string(bx, y, "\u{2502}", border_style); // │
            let mut x = bx + 1;

            for col in 0..fw {
                let is_head = head == Some((row, col));
                let is_body = !is_head && self.body.iter().any(|&(br, bc)| br == row && bc == col);
                let is_food = self.food == (row, col);

                if is_head {
                    buf.set_string(x, y, "\u{2588}\u{2588}", head_style);
                } else if is_body {
                    buf.set_string(x, y, "\u{2593}\u{2593}", body_style);
                } else if is_food {
                    buf.set_string(x, y, "\u{25cf} ", food_style);
                } else {
                    buf.set_string(x, y, "  ", dim);
                }
                x += cell_w;
            }

            buf.set_string(x, y, "\u{2502}", border_style); // │
            y += 1;
        }

        // Bottom border
        if y < y_max {
            let mut border = String::new();
            border.push('\u{2514}'); // └
            for _ in 0..fw as u16 * cell_w {
                border.push('\u{2500}'); // ─
            }
            border.push('\u{2518}'); // ┘
            buf.set_string(bx, y, &border, border_style);
            y += 1;
        }

        // Status
        if self.game_over && y < y_max {
            buf.set_string(
                area.x + 1,
                y,
                "Game Over! ",
                Style::default().fg(Color::Red),
            );
            let sc = format!("Score: {}  ", self.score);
            buf.set_string(area.x + 12, y, &sc, white);
            buf.set_string(
                area.x + 12 + sc.len() as u16,
                y,
                "Press Enter to retry.",
                dim,
            );
        }
    }

    fn game_desired_height(&self, _width: u16) -> u16 {
        // 1 header + 1 top border + 15 rows + 1 bottom border + 1 status = 19
        19
    }

    fn title(&self) -> &str {
        " Snake "
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_does_not_self_collide_on_first_tick() {
        let mut game = SnakeGame::new(FrameRequester::test_dummy());
        game.last_tick = Instant::now() - Duration::from_millis(TICK_MS + 1);

        game.tick();

        assert!(!game.game_over);
    }

    #[test]
    fn snake_moves_right_by_default() {
        let mut game = SnakeGame::new(FrameRequester::test_dummy());
        let (_, head_col_before) = *game.body.back().expect("head exists");
        game.last_tick = Instant::now() - Duration::from_millis(TICK_MS + 1);

        game.tick();

        let (_, head_col_after) = *game.body.back().expect("head exists");
        assert_eq!(head_col_after, head_col_before + 1);
    }
}
