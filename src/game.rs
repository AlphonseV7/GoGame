use wasm_bindgen::prelude::*;
use crate::board::{Board, Color};
use crate::ai;

#[wasm_bindgen]
#[derive(Clone)]
pub struct Game {
    board: Board,
    current_player: Color,
    black_captures: usize,
    white_captures: usize,
    ko_point: Option<(usize, usize)>,
    consecutive_passes: usize,
    game_over: bool,
}

#[wasm_bindgen]
impl Game {
    #[wasm_bindgen(constructor)]
    pub fn new(size: usize) -> Game {
        Game {
            board: Board::new(size),
            current_player: Color::Black,
            black_captures: 0,
            white_captures: 0,
            ko_point: None,
            consecutive_passes: 0,
            game_over: false,
        }
    }

    pub fn place_stone(&mut self, row: usize, col: usize) -> bool {
        let size = self.board.size();
        if self.game_over { return false; }
        if row >= size || col >= size { return false; }
        if self.board.get(row, col) != Color::Empty { return false; }
        if self.ko_point == Some((row, col)) { return false; }

        let mut candidate = self.board.clone();
        candidate.set(row, col, self.current_player);
        let opponent = self.current_player.opposite();
        let captured = candidate.remove_captured(opponent);

        if !candidate.group_has_liberty(row, col) { return false; }

        let new_ko = if captured == 1 && candidate.count_liberties(row, col) == 1 {
            candidate.neighbors(row, col).into_iter()
                .find(|&(nr, nc)| candidate.get(nr, nc) == Color::Empty)
        } else {
            None
        };

        self.board = candidate;
        self.ko_point = new_ko;
        self.consecutive_passes = 0;
        match self.current_player {
            Color::Black => self.black_captures += captured,
            Color::White => self.white_captures += captured,
            Color::Empty => {}
        }
        self.current_player = opponent;
        true
    }

    pub fn pass_turn(&mut self) {
        if self.game_over { return; }
        self.consecutive_passes += 1;
        self.ko_point = None;
        if self.consecutive_passes >= 2 { self.game_over = true; }
        self.current_player = self.current_player.opposite();
    }

    pub fn get_cell(&self, row: usize, col: usize) -> u8 {
        match self.board.get(row, col) {
            Color::Empty => 0, Color::Black => 1, Color::White => 2,
        }
    }

    pub fn current_player(&self) -> u8 {
        match self.current_player { Color::Black => 1, Color::White => 2, Color::Empty => 0 }
    }

    pub fn black_captures(&self) -> usize { self.black_captures }
    pub fn white_captures(&self) -> usize { self.white_captures }
    pub fn is_game_over(&self) -> bool { self.game_over }
    pub fn board_size(&self) -> usize { self.board.size() }

    /// Returns AI move as row*board_size+col, or -1 for pass.
    /// difficulty: 0=noob 1=average 2=dan  |  seed: Date.now() from JS
    pub fn get_ai_move(&self, difficulty: u8, seed: u32) -> i32 {
        ai::get_move(self, difficulty, seed)
    }
}

impl Game {
    pub(crate) fn board(&self) -> &Board { &self.board }
    pub(crate) fn current_player_color(&self) -> Color { self.current_player }

    pub(crate) fn get_legal_moves(&self) -> Vec<(usize, usize)> {
        let size = self.board.size();
        (0..size).flat_map(|r| (0..size).map(move |c| (r, c)))
            .filter(|&(r, c)| self.is_legal(r, c))
            .collect()
    }

    fn is_legal(&self, row: usize, col: usize) -> bool {
        if self.game_over { return false; }
        if self.board.get(row, col) != Color::Empty { return false; }
        if self.ko_point == Some((row, col)) { return false; }
        let mut candidate = self.board.clone();
        candidate.set(row, col, self.current_player);
        candidate.remove_captured(self.current_player.opposite());
        candidate.group_has_liberty(row, col)
    }

    pub(crate) fn score_for(&self, player: Color) -> i32 {
        self.board.count_stones(player) as i32 + match player {
            Color::Black => self.black_captures as i32,
            Color::White => self.white_captures as i32,
            Color::Empty => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_goes_first() { assert_eq!(Game::new(19).current_player(), 1); }

    #[test]
    fn turns_alternate() {
        let mut g = Game::new(19);
        assert!(g.place_stone(9,9));
        assert_eq!(g.current_player(), 2);
        assert!(g.place_stone(3,3));
        assert_eq!(g.current_player(), 1);
    }

    #[test]
    fn cannot_place_on_occupied() {
        let mut g = Game::new(19);
        assert!(g.place_stone(9,9));
        g.place_stone(0,0);
        assert!(!g.place_stone(9,9));
    }

    #[test]
    fn two_passes_end_game() {
        let mut g = Game::new(19);
        g.pass_turn(); g.pass_turn();
        assert!(g.is_game_over());
    }

    #[test]
    fn capture_increments_count() {
        let mut g = Game::new(19);
        g.place_stone(0,1); // Black
        g.place_stone(0,0); // White — corner
        g.place_stone(1,0); // Black — captures white
        assert_eq!(g.black_captures(), 1);
        assert_eq!(g.get_cell(0,0), 0);
    }

    #[test]
    fn suicide_rejected() {
        let mut g = Game::new(19);
        g.place_stone(0,1); g.place_stone(9,9); g.place_stone(1,0);
        assert!(!g.place_stone(0,0)); // White suicide at corner
        assert_eq!(g.current_player(), 2);
    }

    #[test]
    fn game_works_on_9x9() {
        let mut g = Game::new(9);
        assert!(g.place_stone(4,4));
        assert_eq!(g.board_size(), 9);
    }

    #[test]
    fn game_works_on_13x13() {
        let mut g = Game::new(13);
        assert!(g.place_stone(6,6));
        assert_eq!(g.board_size(), 13);
    }
}
