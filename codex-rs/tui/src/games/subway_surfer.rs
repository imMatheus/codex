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

const LANE_COUNT: usize = 3;
const TICK_MS: u64 = 50;
const INITIAL_SPEED: f32 = 0.18;
const MAX_SPEED: f32 = 0.55;
const SPEED_INCREMENT: f32 = 0.00015;
const OBSTACLE_HEIGHT: f32 = 2.0;
const MIN_SPAWN_DISTANCE: f32 = 5.0;
const COIN_SCORE: u32 = 10;
const LANE_WIDTH: u16 = 7;
const FAR_LANE_WIDTH: f32 = 3.0;

// Obstacle color palette - vibrant "train" colors.
const OBSTACLE_COLORS: &[Color] = &[
    Color::Red,     // red train
    Color::Blue,    // blue train
    Color::Yellow,  // orange barrier
    Color::Green,   // green train
    Color::Magenta, // purple train
];

struct Obstacle {
    lane: usize,
    y: f32,
    color: Color,
}

struct Coin {
    lane: usize,
    y: f32,
    collected: bool,
}

pub(crate) struct SubwaySurferGame {
    player_lane: usize,
    obstacles: Vec<Obstacle>,
    coins: Vec<Coin>,
    coins_collected: u32,
    score: u32,
    speed: f32,
    game_over: bool,
    frame_requester: FrameRequester,
    last_tick: Instant,
    distance: f32,
    dist_since_last_spawn: f32,
    spawn_counter: u32,
    ground_scroll: u32,
    /// The visible field height (set on each render).
    field_height: std::cell::Cell<u16>,
}

impl SubwaySurferGame {
    pub fn new(frame_requester: FrameRequester) -> Self {
        Self {
            player_lane: 1, // start center
            obstacles: Vec::new(),
            coins: Vec::new(),
            coins_collected: 0,
            score: 0,
            speed: INITIAL_SPEED,
            game_over: false,
            frame_requester,
            last_tick: Instant::now(),
            distance: 0.0,
            dist_since_last_spawn: 0.0,
            spawn_counter: 0,
            ground_scroll: 0,
            field_height: std::cell::Cell::new(16),
        }
    }

    /// Simple deterministic pseudo-random number from the spawn counter.
    fn pseudo_rand(&mut self) -> u32 {
        self.spawn_counter = self
            .spawn_counter
            .wrapping_mul(1103515245)
            .wrapping_add(12345);
        (self.spawn_counter >> 16) & 0x7FFF
    }

    fn lane_edges_for_row(field_h: u16, row_idx: u16, center_x: f32) -> [f32; LANE_COUNT + 1] {
        let denom = field_h.saturating_sub(1).max(1) as f32;
        let depth = row_idx as f32 / denom;
        let lane_w = FAR_LANE_WIDTH + (LANE_WIDTH as f32 - FAR_LANE_WIDTH) * depth;
        let total_w = lane_w * LANE_COUNT as f32;
        let left = center_x - total_w / 2.0;

        let mut edges = [0.0; LANE_COUNT + 1];
        for (i, edge) in edges.iter_mut().enumerate() {
            *edge = left + lane_w * i as f32;
        }
        edges
    }

    fn lane_columns_from_edges(edges: [f32; LANE_COUNT + 1], area: Rect) -> [u16; LANE_COUNT + 1] {
        let min_x = area.x;
        let max_x = area.x + area.width.saturating_sub(1);
        let mut cols = [0u16; LANE_COUNT + 1];

        for (i, edge) in edges.iter().enumerate() {
            cols[i] = edge.round().clamp(min_x as f32, max_x as f32) as u16;
        }

        for i in 1..=LANE_COUNT {
            let min_allowed = cols[i - 1].saturating_add(2);
            if cols[i] < min_allowed {
                cols[i] = min_allowed.min(max_x);
            }
        }

        cols
    }

    fn spawn_obstacle(&mut self) {
        let lane = self.pseudo_rand() as usize % LANE_COUNT;
        let color_idx = self.pseudo_rand() as usize % OBSTACLE_COLORS.len();
        self.obstacles.push(Obstacle {
            lane,
            y: -OBSTACLE_HEIGHT,
            color: OBSTACLE_COLORS[color_idx],
        });

        // Occasionally spawn a coin in a different lane.
        let coin_chance = self.pseudo_rand() % 3;
        if coin_chance == 0 {
            let mut coin_lane = self.pseudo_rand() as usize % LANE_COUNT;
            if coin_lane == lane {
                coin_lane = (coin_lane + 1) % LANE_COUNT;
            }
            self.coins.push(Coin {
                lane: coin_lane,
                y: -1.0,
                collected: false,
            });
        }
    }

    fn update(&mut self) {
        let fh = self.field_height.get() as f32;
        let player_row = fh - 2.0;

        // Move everything down.
        for obs in &mut self.obstacles {
            obs.y += self.speed;
        }
        for coin in &mut self.coins {
            coin.y += self.speed;
        }

        self.distance += self.speed;
        self.dist_since_last_spawn += self.speed;
        self.ground_scroll = self.ground_scroll.wrapping_add(1);

        // Spawn new obstacles.
        let gap = MIN_SPAWN_DISTANCE + (1.0 - self.speed / MAX_SPEED) * 3.0;
        if self.dist_since_last_spawn >= gap {
            self.dist_since_last_spawn = 0.0;
            self.spawn_obstacle();
        }

        // Score: 1 point per tick survived.
        self.score += 1;

        // Speed up gradually.
        if self.speed < MAX_SPEED {
            self.speed += SPEED_INCREMENT;
            if self.speed > MAX_SPEED {
                self.speed = MAX_SPEED;
            }
        }

        // Collision detection: check obstacles against player.
        for obs in &self.obstacles {
            if obs.lane == self.player_lane {
                let obs_bottom = obs.y + OBSTACLE_HEIGHT;
                if obs_bottom > player_row && obs.y < player_row + 1.0 {
                    self.game_over = true;
                    return;
                }
            }
        }

        // Coin collection.
        for coin in &mut self.coins {
            if !coin.collected && coin.lane == self.player_lane {
                let dy = (coin.y - player_row).abs();
                if dy < 1.0 {
                    coin.collected = true;
                    self.coins_collected += 1;
                    self.score += COIN_SCORE;
                }
            }
        }

        // Remove off-screen objects.
        self.obstacles.retain(|o| o.y < fh + 2.0);
        self.coins.retain(|c| c.y < fh + 2.0);
    }
}

impl GameWidget for SubwaySurferGame {
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
            KeyCode::Left => {
                if self.player_lane > 0 {
                    self.player_lane -= 1;
                }
                true
            }
            KeyCode::Right => {
                if self.player_lane < LANE_COUNT - 1 {
                    self.player_lane += 1;
                }
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
        if now.duration_since(self.last_tick) < Duration::from_millis(TICK_MS) {
            self.frame_requester
                .schedule_frame_in(Duration::from_millis(TICK_MS));
            return;
        }
        self.last_tick = now;
        self.update();
        self.frame_requester
            .schedule_frame_in(Duration::from_millis(TICK_MS));
    }

    fn is_game_over(&self) -> bool {
        self.game_over
    }

    fn reset(&mut self) {
        self.player_lane = 1;
        self.obstacles.clear();
        self.coins.clear();
        self.coins_collected = 0;
        self.score = 0;
        self.speed = INITIAL_SPEED;
        self.game_over = false;
        self.last_tick = Instant::now();
        self.distance = 0.0;
        self.dist_since_last_spawn = 0.0;
        self.ground_scroll = 0;
    }

    fn render_game(&self, area: Rect, buf: &mut Buffer) {
        if area.height < 8 || area.width < 25 {
            return;
        }

        let dim = Style::default().fg(Color::DarkGray);
        let track_border = Style::default().fg(Color::Gray);
        let lane_div = Style::default().fg(Color::DarkGray);
        let white = Style::default().fg(Color::White);
        let ground_dot = Style::default().fg(Color::DarkGray);
        let coin_style = Style::default().fg(Color::Yellow);
        let player_style = Style::default().fg(Color::Cyan);
        let score_label = Style::default().fg(Color::Gray);
        let score_val = Style::default().fg(Color::White);

        let mut y = area.y;
        let y_max = area.y + area.height;

        // Row 0: header
        if y < y_max {
            buf.set_string(area.x + 1, y, "</> to dodge", dim);
            let coins_text = self.coins_collected.to_string();
            let coins_label = "Coins ";
            let score_text = format!("{}", self.score);
            let label = "Score ";
            let score_x = area.x
                + area
                    .width
                    .saturating_sub((label.len() + score_text.len()) as u16 + 1);
            let coins_x = score_x.saturating_sub((coins_label.len() + coins_text.len()) as u16 + 2);
            if coins_x > area.x + 11 {
                buf.set_string(coins_x, y, coins_label, score_label);
                buf.set_string(
                    coins_x + coins_label.len() as u16,
                    y,
                    &coins_text,
                    score_val,
                );
            }
            buf.set_string(score_x, y, label, score_label);
            buf.set_string(score_x + label.len() as u16, y, &score_text, score_val);
            y += 1;
        }

        // Row 1: horizon
        if y < y_max {
            let horizon_w = area.width.saturating_sub(2) as usize;
            let mut horizon = String::with_capacity(horizon_w);
            for i in 0..horizon_w {
                horizon.push(if i % 2 == 0 { '-' } else { '_' });
            }
            buf.set_string(area.x + 1, y, &horizon, track_border);
            y += 1;
        }

        // Game field
        let field_start_y = y;
        let field_h = y_max.saturating_sub(y).saturating_sub(2); // -2 for bottom border + status
        if field_h == 0 {
            return;
        }
        self.field_height.set(field_h);
        let player_row = field_h as f32 - 2.0;
        let center_x = area.x as f32 + area.width as f32 / 2.0;

        for row_idx in 0..field_h {
            let game_row = row_idx as f32;
            let row_y = field_start_y + row_idx;
            if row_y >= y_max {
                break;
            }
            let depth = if field_h > 1 {
                row_idx as f32 / (field_h - 1) as f32
            } else {
                1.0
            };
            let edges = Self::lane_columns_from_edges(
                Self::lane_edges_for_row(field_h, row_idx, center_x),
                area,
            );
            let row_left = edges[0];
            let row_right = edges[LANE_COUNT];
            for x in row_left..=row_right {
                buf.set_string(x, row_y, " ", dim);
            }

            for lane in 0..LANE_COUNT {
                // Determine what to draw in each cell of this lane.
                let lane_left = edges[lane];
                let lane_right = edges[lane + 1];
                if lane_right <= lane_left.saturating_add(1) {
                    continue;
                }

                // Check if an obstacle occupies this (lane, row).
                let mut is_obstacle = false;
                let mut obs_color = Color::Reset;
                for obs in &self.obstacles {
                    if obs.lane == lane {
                        let obs_top = obs.y;
                        let obs_bot = obs.y + OBSTACLE_HEIGHT;
                        if game_row >= obs_top && game_row < obs_bot {
                            is_obstacle = true;
                            obs_color = obs.color;
                            break;
                        }
                    }
                }

                // Check for coin at this (lane, row).
                let mut is_coin = false;
                for coin in &self.coins {
                    if !coin.collected && coin.lane == lane {
                        let dy = (coin.y - game_row).abs();
                        if dy < 0.6 {
                            is_coin = true;
                            break;
                        }
                    }
                }

                // Check for player.
                let is_player = !self.game_over
                    && lane == self.player_lane
                    && (game_row - player_row).abs() < 0.6;

                if is_obstacle {
                    // Draw obstacle block, filled across the lane.
                    let obs_style = Style::default().fg(obs_color);
                    let obstacle_ch = if depth > 0.66 {
                        "@"
                    } else if depth > 0.33 {
                        "#"
                    } else {
                        "x"
                    };
                    for x in lane_left.saturating_add(1)..lane_right {
                        buf.set_string(x, row_y, obstacle_ch, obs_style);
                    }
                } else if is_player {
                    // Draw player character centered in lane.
                    let center = lane_left + (lane_right - lane_left) / 2;
                    if lane_right.saturating_sub(lane_left) >= 4 {
                        buf.set_string(center.saturating_sub(1), row_y, "<", player_style);
                        buf.set_string(center, row_y, "A", player_style);
                        buf.set_string(center.saturating_add(1), row_y, ">", player_style);
                    } else {
                        buf.set_string(center, row_y, "A", player_style);
                    }
                } else if is_coin {
                    // Draw coin centered in lane.
                    let center = lane_left + (lane_right - lane_left) / 2;
                    buf.set_string(center, row_y, "$", coin_style);
                } else {
                    // Empty lane - draw ground pattern.
                    let scroll = self.ground_scroll as u16;
                    if (row_idx + scroll).is_multiple_of(3) {
                        let center = lane_left + (lane_right - lane_left) / 2;
                        buf.set_string(center, row_y, ".", ground_dot);
                    }
                }
            }

            for (edge_idx, edge_x) in edges.iter().enumerate().take(LANE_COUNT + 1) {
                let edge_style = if edge_idx == 0 || edge_idx == LANE_COUNT {
                    track_border
                } else {
                    lane_div
                };
                let edge_char = if edge_idx <= LANE_COUNT / 2 {
                    "/"
                } else {
                    "\\"
                };
                buf.set_string(*edge_x, row_y, edge_char, edge_style);
            }
        }
        y = field_start_y + field_h;

        // Bottom track border
        if y < y_max {
            let bottom_edges = Self::lane_columns_from_edges(
                Self::lane_edges_for_row(field_h, field_h.saturating_sub(1), center_x),
                area,
            );
            for x in bottom_edges[0]..=bottom_edges[LANE_COUNT] {
                buf.set_string(x, y, "-", track_border);
            }
            y += 1;
        }

        // Status line
        if y < y_max && self.game_over {
            buf.set_string(area.x + 1, y, "Crashed! ", Style::default().fg(Color::Red));
            let sc = format!("Score: {}  Coins: {}  ", self.score, self.coins_collected);
            buf.set_string(area.x + 10, y, &sc, white);
            buf.set_string(
                area.x + 10 + sc.len() as u16,
                y,
                "Press Enter to retry.",
                dim,
            );
        }

        // If player just died, render a crash effect on the player position.
        if self.game_over {
            let crash_row = player_row.max(0.0).min(field_h.saturating_sub(1) as f32) as u16;
            let edges = Self::lane_columns_from_edges(
                Self::lane_edges_for_row(field_h, crash_row, center_x),
                area,
            );
            let lane_left = edges[self.player_lane];
            let lane_right = edges[self.player_lane + 1];
            let px = lane_left + (lane_right - lane_left) / 2;
            let py = field_start_y + crash_row;
            if py < y_max && px < area.x + area.width {
                buf.set_string(
                    px.saturating_sub(1),
                    py,
                    "XXX",
                    Style::default().fg(Color::Red),
                );
            }
        }
    }

    fn game_desired_height(&self, _width: u16) -> u16 {
        // 1 header + 1 top border + 16 field rows + 1 bottom border + 1 status = 20
        20
    }

    fn title(&self) -> &str {
        " Subway Surfer "
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn collecting_coin_updates_coin_counter_and_score_once() {
        let mut game = SubwaySurferGame::new(FrameRequester::test_dummy());
        game.coins.push(Coin {
            lane: game.player_lane,
            y: 14.0,
            collected: false,
        });

        game.update();
        assert_eq!(game.coins_collected, 1);
        assert_eq!(game.score, COIN_SCORE + 1);
        assert_eq!(game.coins[0].collected, true);

        game.update();
        assert_eq!(game.coins_collected, 1);
        assert_eq!(game.score, COIN_SCORE + 2);
    }
}
