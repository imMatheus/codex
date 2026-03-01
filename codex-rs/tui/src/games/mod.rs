pub(crate) mod connect4;
pub(crate) mod flappy_bird;
pub(crate) mod snake;
pub(crate) mod subway_surfer;
pub(crate) mod tetris;
pub(crate) mod wordle;

use codex_core::config::types::MiniGameKind;
use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::WidgetRef;

use crate::render::renderable::Renderable;
use crate::tui::FrameRequester;

/// Trait that each mini-game implements.
pub(crate) trait GameWidget {
    /// Handle a key event directed at the game. Returns true if the key was consumed.
    fn handle_key_event(&mut self, key_event: KeyEvent) -> bool;

    /// Called each animation tick to advance game state for real-time games.
    fn tick(&mut self);

    /// Whether the game has concluded (win/loss/draw).
    #[allow(dead_code)]
    fn is_game_over(&self) -> bool;

    /// Reset the game to start a new round.
    fn reset(&mut self);

    /// Render the game within the given area.
    fn render_game(&self, area: Rect, buf: &mut Buffer);

    /// The desired height for the game area.
    fn game_desired_height(&self, width: u16) -> u16;

    /// The title to display in the game border.
    fn title(&self) -> &str;
}

/// Container that owns the currently active game instance.
pub(crate) struct GameOverlay {
    game: Box<dyn GameWidget>,
}

impl GameOverlay {
    pub fn new(kind: MiniGameKind, frame_requester: FrameRequester) -> Self {
        let game: Box<dyn GameWidget> = match kind {
            MiniGameKind::Connect4 => Box::new(connect4::Connect4Game::new(frame_requester)),
            MiniGameKind::Tetris => Box::new(tetris::TetrisGame::new(frame_requester)),
            MiniGameKind::Wordle => Box::new(wordle::WordleGame::new()),
            MiniGameKind::SubwaySurfer => {
                Box::new(subway_surfer::SubwaySurferGame::new(frame_requester))
            }
            MiniGameKind::Snake => Box::new(snake::SnakeGame::new(frame_requester)),
            MiniGameKind::FlappyBird => Box::new(flappy_bird::FlappyBirdGame::new(frame_requester)),
        };
        Self { game }
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> bool {
        self.game.handle_key_event(key_event)
    }

    pub fn tick(&mut self) {
        self.game.tick();
    }
}

impl Renderable for GameOverlay {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.height < 4 || area.width < 10 {
            return;
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(self.game.title());

        let inner = block.inner(area);
        block.render_ref(area, buf);
        self.game.render_game(inner, buf);
    }

    fn desired_height(&self, width: u16) -> u16 {
        // Add 2 for the border
        self.game.game_desired_height(width) + 2
    }
}
