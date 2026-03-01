use std::cell::Cell;
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

// Timing
const FRAME_MS: u64 = 16;
const MAX_DT: f32 = 0.05;

// Bird physics
const GRAVITY: f32 = 20.0;
const FLAP_VY: f32 = -8.0;
const MAX_FALL_VY: f32 = 8.0;

// Pipes
const PIPE_SPEED: f32 = 14.0;
const PIPE_GAP: f32 = 6.0;
const PIPE_WIDTH: f32 = 3.0;
const PIPE_SPACING: f32 = 20.0;
const GAP_TOP_MIN: f32 = 2.0;
const GAP_BOT_MIN: f32 = 2.0;

// Layout
const BIRD_COL: f32 = 8.0;
const BIRD_START_Y: f32 = 6.0;
const FIELD_ROWS: u16 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Idle,
    Playing,
    Dead,
}

struct Pipe {
    x: f32,
    gap_y: f32,
    scored: bool,
}

pub(crate) struct FlappyBirdGame {
    y: f32,
    vy: f32,
    pipes: Vec<Pipe>,
    score: u32,
    state: State,
    last_tick: Instant,
    frame_requester: FrameRequester,
    field_w: Cell<f32>,
    field_h: Cell<f32>,
    rng: u32,
}

impl FlappyBirdGame {
    pub fn new(frame_requester: FrameRequester) -> Self {
        let mut game = Self {
            y: BIRD_START_Y,
            vy: 0.0,
            pipes: Vec::new(),
            score: 0,
            state: State::Idle,
            last_tick: Instant::now(),
            frame_requester,
            field_w: Cell::new(200.0),
            field_h: Cell::new(FIELD_ROWS as f32),
            rng: 0xDEAD_BEEF,
        };
        game.spawn_pipes_if_needed();
        game
    }

    // ── RNG (xorshift32) ──

    fn rand_u32(&mut self) -> u32 {
        let mut s = self.rng;
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        self.rng = s;
        s
    }

    fn rand_range(&mut self, lo: f32, hi: f32) -> f32 {
        if hi <= lo {
            return lo;
        }
        let t = (self.rand_u32() % 10000) as f32 / 10000.0;
        lo + t * (hi - lo)
    }

    // ── Pipe helpers ──

    fn max_gap_y(&self) -> f32 {
        (self.field_h.get() - PIPE_GAP - GAP_BOT_MIN).max(GAP_TOP_MIN)
    }

    fn make_pipe(&mut self, x: f32) {
        let gap_y = self.rand_range(GAP_TOP_MIN, self.max_gap_y());
        self.pipes.push(Pipe {
            x,
            gap_y,
            scored: false,
        });
    }

    /// Spawn pipes at the right edge whenever the rightmost pipe has scrolled
    /// far enough to leave a `PIPE_SPACING`-sized gap.
    ///
    /// On the very first call (no pipes yet) the first pipe is placed roughly
    /// one-third of the way across the field so pipes feel close from the start,
    /// then additional pipes fill out to the right edge at regular spacing.
    fn spawn_pipes_if_needed(&mut self) {
        let edge = self.field_w.get() + PIPE_WIDTH;

        if self.pipes.is_empty() {
            // Seed the first pipe closer to the bird.
            let first_x = (BIRD_COL + PIPE_SPACING).min(self.field_w.get() * 0.4);
            self.make_pipe(first_x);
        }

        while self
            .pipes
            .last()
            .map_or(true, |p| edge - p.x >= PIPE_SPACING)
        {
            let next_x = self.pipes.last().map_or(edge, |p| p.x + PIPE_SPACING);
            self.make_pipe(next_x);
        }
    }

    // ── Physics ──

    fn step(&mut self, dt: f32) {
        if self.state != State::Playing {
            return;
        }

        let fh = self.field_h.get();

        // Gravity -> velocity -> position
        self.vy = (self.vy + GRAVITY * dt).min(MAX_FALL_VY);
        self.y += self.vy * dt;

        // Ceiling clamp
        if self.y < 0.0 {
            self.y = 0.0;
            self.vy = 0.0;
        }

        // Floor -> death
        if self.y >= fh - 1.0 {
            self.y = fh - 1.0;
            self.state = State::Dead;
            return;
        }

        // Scroll pipes left
        for p in &mut self.pipes {
            p.x -= PIPE_SPEED * dt;
        }

        // Cull off-screen pipes
        self.pipes.retain(|p| p.x > -(PIPE_WIDTH + 1.0));

        // Spawn new pipes at the right edge
        self.spawn_pipes_if_needed();

        // Collision detection & scoring
        for p in &mut self.pipes {
            let pr = p.x + PIPE_WIDTH;

            // Score when pipe fully passes the bird
            if !p.scored && pr < BIRD_COL {
                p.scored = true;
                self.score += 1;
            }

            // Horizontal overlap?
            if BIRD_COL + 1.0 > p.x && BIRD_COL < pr {
                let gap_bot = p.gap_y + PIPE_GAP;
                if self.y < p.gap_y || self.y + 1.0 > gap_bot {
                    self.state = State::Dead;
                    return;
                }
            }
        }
    }
}

impl GameWidget for FlappyBirdGame {
    fn handle_key_event(&mut self, ev: KeyEvent) -> bool {
        if ev.kind != KeyEventKind::Press {
            return false;
        }

        match self.state {
            State::Dead => {
                if matches!(ev.code, KeyCode::Enter | KeyCode::Char(' ')) {
                    self.reset();
                    return true;
                }
                false
            }
            State::Idle => match ev.code {
                KeyCode::Up | KeyCode::Char(' ') | KeyCode::Enter => {
                    self.state = State::Playing;
                    self.last_tick = Instant::now();
                    self.vy = FLAP_VY;
                    self.spawn_pipes_if_needed();
                    self.frame_requester
                        .schedule_frame_in(Duration::from_millis(FRAME_MS));
                    true
                }
                _ => false,
            },
            State::Playing => match ev.code {
                KeyCode::Up | KeyCode::Char(' ') | KeyCode::Enter => {
                    self.vy = FLAP_VY;
                    true
                }
                _ => false,
            },
        }
    }

    fn tick(&mut self) {
        if self.state != State::Playing {
            return;
        }
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f32().min(MAX_DT);
        self.last_tick = now;
        self.step(dt);
        if self.state == State::Playing {
            self.frame_requester
                .schedule_frame_in(Duration::from_millis(FRAME_MS));
        }
    }

    fn is_game_over(&self) -> bool {
        self.state == State::Dead
    }

    fn reset(&mut self) {
        self.y = BIRD_START_Y;
        self.vy = 0.0;
        self.score = 0;
        self.state = State::Idle;
        self.last_tick = Instant::now();
        self.pipes.clear();
        self.spawn_pipes_if_needed();
    }

    fn render_game(&self, area: Rect, buf: &mut Buffer) {
        if area.height < 5 || area.width < 20 {
            return;
        }

        // Compute field dimensions from available area.
        let field_h = area.height.saturating_sub(3) as f32;
        let field_w = area.width.saturating_sub(2) as f32;
        self.field_h.set(field_h);
        self.field_w.set(field_w);

        let field_rows = field_h as u16;
        let field_cols = field_w as u16;
        let lx = area.x;
        let ix = area.x + 1;
        let rx = area.x + area.width - 1;
        let mut cy = area.y;
        let y_end = area.y + area.height;

        // Styles
        let dim = Style::default().fg(Color::DarkGray);
        let white = Style::default().fg(Color::White);
        let gray = Style::default().fg(Color::Gray);
        let border_s = Style::default().fg(Color::DarkGray);
        let bird_s = Style::default().fg(Color::Yellow);
        let dead_s = Style::default().fg(Color::Red);
        let pipe_s = Style::default().fg(Color::Green);
        let cap_s = Style::default().fg(Color::LightGreen);
        let sky_s = Style::default().fg(Color::Blue);
        let ground_s = Style::default().fg(Color::Yellow);

        // ── Header ──
        if cy < y_end {
            let hint = match self.state {
                State::Idle => "Press Space / Up to start!",
                State::Playing => "Space / Up to flap",
                State::Dead => "Game Over!",
            };
            buf.set_string(ix, cy, hint, dim);
            let sc = format!("Score: {}", self.score);
            let sx = rx.saturating_sub(sc.len() as u16);
            buf.set_string(sx, cy, &sc, gray);
            cy += 1;
        }

        // ── Game field ──
        let bird_row = self.y.round().clamp(0.0, field_h - 1.0) as u16;
        let bird_col = BIRD_COL as u16;

        for row in 0..field_rows {
            if cy >= y_end {
                break;
            }

            buf.set_string(lx, cy, "\u{2502}", border_s);

            for col in 0..field_cols {
                let px = ix + col;
                if px >= rx {
                    break;
                }

                // Bird
                if col == bird_col && row == bird_row {
                    if self.state == State::Dead {
                        buf.set_string(px, cy, "\u{2716}", dead_s);
                    } else if self.vy < -1.0 {
                        buf.set_string(px, cy, "\u{25b2}", bird_s);
                    } else if self.vy > 1.0 {
                        buf.set_string(px, cy, "\u{25bc}", bird_s);
                    } else {
                        buf.set_string(px, cy, "\u{25cf}", bird_s);
                    }
                    continue;
                }

                // Pipe check
                let r = row as f32;
                let c = col as f32;
                let mut drew_pipe = false;
                for p in &self.pipes {
                    if c >= p.x && c < p.x + PIPE_WIDTH {
                        let gap_bot = p.gap_y + PIPE_GAP;
                        if r < p.gap_y || r >= gap_bot {
                            let top_cap = (p.gap_y.ceil() - 1.0) as u16;
                            let bot_cap = gap_bot.ceil() as u16;
                            if row == top_cap || row == bot_cap {
                                buf.set_string(px, cy, "\u{2588}", cap_s);
                            } else {
                                buf.set_string(px, cy, "\u{2593}", pipe_s);
                            }
                            drew_pipe = true;
                            break;
                        }
                    }
                }
                if drew_pipe {
                    continue;
                }

                // Sky
                let star = (row.wrapping_mul(7).wrapping_add(col.wrapping_mul(13))) % 41 == 0;
                if star {
                    buf.set_string(px, cy, "\u{00b7}", sky_s);
                } else {
                    buf.set_string(px, cy, " ", dim);
                }
            }

            buf.set_string(rx, cy, "\u{2502}", border_s);
            cy += 1;
        }

        // ── Ground ──
        if cy < y_end {
            buf.set_string(lx, cy, "\u{2502}", border_s);
            for col in 0..field_cols {
                let px = ix + col;
                if px >= rx {
                    break;
                }
                let ch = if col % 3 == 0 { "\u{2584}" } else { "\u{2580}" };
                buf.set_string(px, cy, ch, ground_s);
            }
            buf.set_string(rx, cy, "\u{2502}", border_s);
            cy += 1;
        }

        // ── Status (game-over) ──
        if cy < y_end && self.state == State::Dead {
            let msg = format!("Score: {}  |  Press Space / Enter to retry", self.score);
            buf.set_string(ix, cy, &msg, white);
        }
    }

    fn game_desired_height(&self, _width: u16) -> u16 {
        1 + FIELD_ROWS + 1 + 1
    }

    fn title(&self) -> &str {
        " Flappy Bird "
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;
    use pretty_assertions::assert_eq;

    use super::*;

    fn make_game() -> FlappyBirdGame {
        FlappyBirdGame::new(FrameRequester::test_dummy())
    }

    fn press(game: &mut FlappyBirdGame, code: KeyCode) -> bool {
        game.handle_key_event(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn starts_with_pipes_visible() {
        let game = make_game();
        assert!(
            game.pipes.len() >= 2,
            "pipes should be seeded at construction"
        );
        assert!(
            game.pipes[0].x > BIRD_COL,
            "first pipe should be ahead of bird"
        );
    }

    #[test]
    fn flap_starts_game() {
        let mut game = make_game();
        assert_eq!(game.state, State::Idle);
        assert!(press(&mut game, KeyCode::Char(' ')));
        assert_eq!(game.state, State::Playing);
        assert_eq!(game.vy, FLAP_VY);
    }

    #[test]
    fn small_step_does_not_spawn_extra_pipes() {
        let mut game = make_game();
        press(&mut game, KeyCode::Up);
        // First step seeds pipes from the bird out to the right edge.
        game.step(0.01);
        let n = game.pipes.len();
        assert!(n >= 2, "initial seeding should place several pipes");
        // A small subsequent step should not add more.
        game.step(0.01);
        assert_eq!(game.pipes.len(), n);
    }

    #[test]
    fn pipes_spawn_when_field_empties() {
        let mut game = make_game();
        press(&mut game, KeyCode::Up);
        game.pipes.clear();
        game.step(0.01);
        assert!(!game.pipes.is_empty(), "should spawn when empty");
    }

    #[test]
    fn bird_dies_on_floor() {
        let mut game = make_game();
        game.state = State::Playing;
        game.y = game.field_h.get() - 2.0;
        game.vy = 5.0;
        game.pipes.clear();
        game.step(0.5);
        assert_eq!(game.state, State::Dead);
    }

    #[test]
    fn reset_restores_idle() {
        let mut game = make_game();
        press(&mut game, KeyCode::Char(' '));
        game.score = 5;
        game.reset();
        assert_eq!(game.state, State::Idle);
        assert_eq!(game.y, BIRD_START_Y);
        assert_eq!(game.score, 0);
        assert!(!game.pipes.is_empty(), "reset should re-seed pipes");
    }
}
