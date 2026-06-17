use crate::game::Game;
use crate::board::{Board, Color};

struct Rng { state: u64 }

impl Rng {
    fn new(seed: u32) -> Self {
        let s = (seed as u64 ^ 0x9e3779b97f4a7c15).wrapping_mul(6364136223846793005);
        Rng { state: if s == 0 { 12345 } else { s } }
    }
    fn next(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }
    fn pick(&mut self, slice: &[(usize, usize)]) -> (usize, usize) {
        slice[(self.next() as usize) % slice.len()]
    }
    fn shuffle(&mut self, v: &mut Vec<(usize, usize)>) {
        for i in (1..v.len()).rev() {
            let j = (self.next() as usize) % (i + 1);
            v.swap(i, j);
        }
    }
}

pub fn get_move(game: &Game, difficulty: u8, seed: u32) -> i32 {
    let mut rng = Rng::new(seed);
    let moves = game.get_legal_moves();
    if moves.is_empty() { return -1; }
    let size = game.board_size();
    let (row, col) = match difficulty {
        0 => noob(&moves, &mut rng),
        1 => average(game, &moves, &mut rng),
        _ => dan(game, &moves, &mut rng),
    };
    (row * size + col) as i32
}

// ---- Noob: any legal move at random ----
fn noob(moves: &[(usize, usize)], rng: &mut Rng) -> (usize, usize) {
    rng.pick(moves)
}

// ---- Average: simple tactics, avoids the dumbest moves ----
fn average(game: &Game, moves: &[(usize, usize)], rng: &mut Rng) -> (usize, usize) {
    // 1. Capture an opponent group if we can.
    if let Some(&m) = moves.iter().find(|&&(r, c)| captures_opponent(game, r, c)) {
        return m;
    }
    // 2. Save one of our own groups that is in atari (without self-atari).
    if let Some(&m) = moves.iter()
        .find(|&&(r, c)| saves_atari(game, r, c) && !is_self_atari(game, r, c)) {
        return m;
    }
    // 3. Otherwise play a "sensible" move: not self-atari, not filling our own eye.
    let board = game.board();
    let me = game.current_player_color();
    let good: Vec<(usize, usize)> = moves.iter()
        .filter(|&&(r, c)| !is_self_atari(game, r, c) && !is_eye_for(board, r, c, me))
        .copied().collect();
    let pool = if good.is_empty() { moves.to_vec() } else { good };
    // Prefer moves that touch an existing stone (keeps play connected/relevant).
    let contact: Vec<(usize, usize)> = pool.iter()
        .filter(|&&(r, c)| board.neighbors(r, c).iter()
            .any(|&(nr, nc)| board.get(nr, nc) != Color::Empty))
        .copied().collect();
    if !contact.is_empty() { rng.pick(&contact) } else { rng.pick(&pool) }
}

// ---- Dan: Monte Carlo with eye-aware playouts ----
fn dan(game: &Game, moves: &[(usize, usize)], rng: &mut Rng) -> (usize, usize) {
    let size = game.board_size();
    let me = game.current_player_color();
    let (max_cands, sims) = match size {
        n if n >= 19 => (30, 14),
        n if n >= 13 => (40, 25),
        _            => (60, 45),
    };

    // Free capture that doesn't put us in atari: just take it.
    if let Some(&m) = moves.iter()
        .find(|&&(r, c)| captures_opponent(game, r, c) && !is_self_atari(game, r, c)) {
        return m;
    }

    // Build an ordered candidate list, dropping clearly-bad moves.
    let board = game.board();
    let tactical: Vec<(usize, usize)> = moves.iter()
        .filter(|&&(r, c)| captures_opponent(game, r, c) || saves_atari(game, r, c))
        .copied().collect();
    let mut contact: Vec<(usize, usize)> = moves.iter()
        .filter(|&&(r, c)| !tactical.contains(&(r, c))
            && !is_self_atari(game, r, c) && !is_eye_for(board, r, c, me)
            && board.neighbors(r, c).iter().any(|&(nr, nc)| board.get(nr, nc) != Color::Empty))
        .copied().collect();
    rng.shuffle(&mut contact);
    let mut rest: Vec<(usize, usize)> = moves.iter()
        .filter(|&&(r, c)| !tactical.contains(&(r, c)) && !contact.contains(&(r, c))
            && !is_self_atari(game, r, c) && !is_eye_for(board, r, c, me))
        .copied().collect();
    rng.shuffle(&mut rest);

    let mut cands = tactical;
    cands.extend_from_slice(&contact);
    cands.extend_from_slice(&rest);
    cands.truncate(max_cands);
    if cands.is_empty() { return noob(moves, rng); }

    let mut best = cands[0];
    let mut best_score = -1.0f64;
    for &(r, c) in &cands {
        let mut wins = 0.0;
        let mut n = 0.0;
        for _ in 0..sims {
            if let Some(res) = playout(game, (r, c), me, rng) {
                wins += res;
                n += 1.0;
            }
        }
        let score = if n > 0.0 { wins / n } else { -1.0 };
        if score > best_score { best_score = score; best = (r, c); }
    }
    best
}

/// Play one random-but-eye-respecting game to the end. Returns 1.0 win / 0.5 tie / 0.0 loss
/// for `me`, or None if the seeding move was illegal.
fn playout(start: &Game, first: (usize, usize), me: Color, rng: &mut Rng) -> Option<f64> {
    let mut sim = start.clone();
    if !sim.place_stone(first.0, first.1) { return None; }
    let size = sim.board_size();
    let max_moves = size * size * 2;
    let mut played = 0;

    while !sim.is_game_over() && played < max_moves {
        let player = sim.current_player_color();
        let mut placed = false;
        // Try a handful of random points; skip our own eyes so groups stay alive.
        for _ in 0..10 {
            let r = (rng.next() as usize) % size;
            let c = (rng.next() as usize) % size;
            if sim.board().get(r, c) != Color::Empty { continue; }
            if is_eye_for(sim.board(), r, c, player) { continue; }
            if sim.place_stone(r, c) { placed = true; break; }
        }
        if !placed { sim.pass_turn(); }
        played += 1;
    }

    let mine = area_score(sim.board(), me);
    let theirs = area_score(sim.board(), me.opposite());
    Some(if mine > theirs { 1.0 } else if mine < theirs { 0.0 } else { 0.5 })
}

/// Area score (Tromp-Taylor style): stones of `color` + empty points reachable
/// only from `color`.
fn area_score(board: &Board, color: Color) -> i32 {
    let size = board.size();
    let mut visited = vec![vec![false; size]; size];
    let mut score = 0i32;

    for r in 0..size {
        for c in 0..size {
            let cell = board.get(r, c);
            if cell == color {
                score += 1;
            } else if cell == Color::Empty && !visited[r][c] {
                // Flood-fill this empty region, tracking which colors border it.
                let mut region = Vec::new();
                let mut stack = vec![(r, c)];
                let mut borders_me = false;
                let mut borders_other = false;
                visited[r][c] = true;
                while let Some((cr, cc)) = stack.pop() {
                    region.push((cr, cc));
                    for (nr, nc) in board.neighbors(cr, cc) {
                        match board.get(nr, nc) {
                            Color::Empty => {
                                if !visited[nr][nc] {
                                    visited[nr][nc] = true;
                                    stack.push((nr, nc));
                                }
                            }
                            c2 if c2 == color => borders_me = true,
                            _ => borders_other = true,
                        }
                    }
                }
                if borders_me && !borders_other {
                    score += region.len() as i32;
                }
            }
        }
    }
    score
}

// ---- Tactical helpers ----

fn captures_opponent(game: &Game, row: usize, col: usize) -> bool {
    let board = game.board();
    let opp = game.current_player_color().opposite();
    board.neighbors(row, col).iter().any(|&(nr, nc)| {
        board.get(nr, nc) == opp && board.count_liberties(nr, nc) == 1
    })
}

fn saves_atari(game: &Game, row: usize, col: usize) -> bool {
    let board = game.board();
    let cur = game.current_player_color();
    board.neighbors(row, col).iter().any(|&(nr, nc)| {
        board.get(nr, nc) == cur && board.count_liberties(nr, nc) == 1
    })
}

/// Would placing here leave our own stone/group with <= 1 liberty (after captures)?
fn is_self_atari(game: &Game, row: usize, col: usize) -> bool {
    let mut b = game.board().clone();
    let me = game.current_player_color();
    b.set(row, col, me);
    b.remove_captured(me.opposite());
    b.count_liberties(row, col) <= 1
}

/// Simple eye test: empty point whose on-board orthogonal neighbors are all `color`.
fn is_eye_for(board: &Board, row: usize, col: usize, color: Color) -> bool {
    if board.get(row, col) != Color::Empty { return false; }
    board.neighbors(row, col).iter().all(|&(nr, nc)| board.get(nr, nc) == color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eye_detection_center() {
        let mut g = Game::new(9);
        // Surround (4,4) with black stones on all four sides.
        let mut b = g.board().clone();
        b.set(3, 4, Color::Black);
        b.set(5, 4, Color::Black);
        b.set(4, 3, Color::Black);
        b.set(4, 5, Color::Black);
        assert!(is_eye_for(&b, 4, 4, Color::Black));
        assert!(!is_eye_for(&b, 4, 4, Color::White));
        let _ = &mut g;
    }

    #[test]
    fn area_score_counts_territory() {
        let mut b = Board::new(9);
        // A single black stone in the corner: it owns nothing but itself unless
        // it fully encloses empty space. Just verify stone counting works.
        b.set(0, 0, Color::Black);
        assert!(area_score(&b, Color::Black) >= 1);
    }

    #[test]
    fn dan_returns_legal_move_on_empty_board() {
        let g = Game::new(9);
        let mv = get_move(&g, 2, 42);
        assert!(mv >= 0);
        let size = g.board_size() as i32;
        assert!(mv < size * size);
    }

    #[test]
    fn dan_takes_free_capture() {
        let mut g = Game::new(9);
        // Set up a white stone in atari that black can capture.
        g.place_stone(0, 1); // Black
        g.place_stone(0, 0); // White, corner, 1 liberty left at (1,0)
        // Black to move; (1,0) captures.
        let mv = get_move(&g, 2, 7);
        assert_eq!(mv, (1 * 9 + 0) as i32);
    }
}
