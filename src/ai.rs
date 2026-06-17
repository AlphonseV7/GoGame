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
}

pub fn get_move(game: &Game, difficulty: u8, seed: u32) -> i32 {
    let mut rng = Rng::new(seed);
    let moves = game.get_legal_moves();
    if moves.is_empty() { return -1; }
    let size = game.board_size();
    match difficulty {
        0 => { let (r, c) = noob(&moves, &mut rng); (r * size + c) as i32 }
        1 => { let (r, c) = average(game, &moves, &mut rng); (r * size + c) as i32 }
        _ => dan_index(game, &moves, &mut rng),
    }
}

// ── Noob: any legal move at random ──
fn noob(moves: &[(usize, usize)], rng: &mut Rng) -> (usize, usize) {
    rng.pick(moves)
}

// ── Average: simple tactics ──
fn average(game: &Game, moves: &[(usize, usize)], rng: &mut Rng) -> (usize, usize) {
    if let Some(&m) = moves.iter().find(|&&(r, c)| captures_opponent(game, r, c)) {
        return m;
    }
    if let Some(&m) = moves.iter()
        .find(|&&(r, c)| saves_atari(game, r, c) && !is_self_atari(game, r, c)) {
        return m;
    }
    let board = game.board();
    let me = game.current_player_color();
    let good: Vec<(usize, usize)> = moves.iter()
        .filter(|&&(r, c)| !is_self_atari(game, r, c) && !is_eye_for(board, r, c, me))
        .copied().collect();
    let pool = if good.is_empty() { moves.to_vec() } else { good };
    let contact: Vec<(usize, usize)> = pool.iter()
        .filter(|&&(r, c)| board.neighbors(r, c).iter()
            .any(|&(nr, nc)| board.get(nr, nc) != Color::Empty))
        .copied().collect();
    if !contact.is_empty() { rng.pick(&contact) } else { rng.pick(&pool) }
}

// ── Dan: Monte Carlo Tree Search (UCT) ──

struct Node {
    game: Game,
    mv: Option<(usize, usize)>,
    parent: Option<usize>,
    children: Vec<usize>,
    untried: Vec<(usize, usize)>,
    wins: f64,
    visits: f64,
    player_just_moved: Color,
    to_move: Color,
}

fn dan_index(game: &Game, moves: &[(usize, usize)], rng: &mut Rng) -> i32 {
    // Take an obvious free capture immediately — no need to search.
    if let Some(&(r, c)) = moves.iter()
        .find(|&&(r, c)| captures_opponent(game, r, c) && !is_self_atari(game, r, c)) {
        return (r * game.board_size() + c) as i32;
    }

    let size = game.board_size();
    let iterations = match size {
        n if n >= 19 => 400,
        n if n >= 13 => 900,
        _            => 1600,
    };
    mcts(game, rng, iterations)
}

fn mcts(game: &Game, rng: &mut Rng, iterations: usize) -> i32 {
    const C: f64 = 1.41; // exploration constant (~sqrt 2)

    let root_cands = sensible_moves(game);
    if root_cands.is_empty() { return -1; }

    let to_move = game.current_player_color();
    let mut nodes: Vec<Node> = vec![Node {
        game: game.clone(),
        mv: None,
        parent: None,
        children: Vec::new(),
        untried: root_cands,
        wins: 0.0,
        visits: 0.0,
        player_just_moved: to_move.opposite(),
        to_move,
    }];

    for _ in 0..iterations {
        // 1. Selection: descend fully-expanded nodes by UCT.
        let mut idx = 0;
        while nodes[idx].untried.is_empty() && !nodes[idx].children.is_empty() {
            let parent_visits = nodes[idx].visits;
            let mut best_child = nodes[idx].children[0];
            let mut best_uct = f64::MIN;
            for &ci in &nodes[idx].children {
                let c = &nodes[ci];
                let uct = c.wins / c.visits + C * (parent_visits.ln() / c.visits).sqrt();
                if uct > best_uct { best_uct = uct; best_child = ci; }
            }
            idx = best_child;
        }

        // 2. Expansion: add one child for an untried move.
        if !nodes[idx].untried.is_empty() {
            let n = nodes[idx].untried.len();
            let mv = nodes[idx].untried.swap_remove((rng.next() as usize) % n);
            let mover = nodes[idx].to_move;
            let mut child_game = nodes[idx].game.clone();
            child_game.place_stone(mv.0, mv.1);
            let child_cands = sensible_moves(&child_game);
            let new_idx = nodes.len();
            nodes.push(Node {
                game: child_game,
                mv: Some(mv),
                parent: Some(idx),
                children: Vec::new(),
                untried: child_cands,
                wins: 0.0,
                visits: 0.0,
                player_just_moved: mover,
                to_move: mover.opposite(),
            });
            nodes[idx].children.push(new_idx);
            idx = new_idx;
        }

        // 3. Simulation: random eye-aware playout to the end.
        let winner = simulate(&nodes[idx].game, rng);

        // 4. Backpropagation.
        let mut cur = Some(idx);
        while let Some(ci) = cur {
            let node = &mut nodes[ci];
            node.visits += 1.0;
            if winner == node.player_just_moved { node.wins += 1.0; }
            else if winner == Color::Empty { node.wins += 0.5; }
            cur = node.parent;
        }
    }

    // Pick the most-visited child of the root (robust choice).
    let mut best_mv = None;
    let mut best_visits = -1.0;
    for &ci in &nodes[0].children {
        if nodes[ci].visits > best_visits {
            best_visits = nodes[ci].visits;
            best_mv = nodes[ci].mv;
        }
    }
    match best_mv {
        Some((r, c)) => (r * game.board_size() + c) as i32,
        None => -1,
    }
}

/// Legal moves minus self-atari and our own eyes.
fn sensible_moves(game: &Game) -> Vec<(usize, usize)> {
    let board = game.board();
    let me = game.current_player_color();
    game.get_legal_moves().into_iter()
        .filter(|&(r, c)| !is_self_atari(game, r, c) && !is_eye_for(board, r, c, me))
        .collect()
}

/// Random eye-aware playout. Returns the winning Color or Empty for a tie.
fn simulate(start: &Game, rng: &mut Rng) -> Color {
    let mut sim = start.clone();
    let size = sim.board_size();
    let max_moves = size * size * 2;
    let mut played = 0;

    while !sim.is_game_over() && played < max_moves {
        let player = sim.current_player_color();
        let mut placed = false;
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

    let b = area_score(sim.board(), Color::Black);
    let w = area_score(sim.board(), Color::White);
    if b > w { Color::Black } else if w > b { Color::White } else { Color::Empty }
}

/// Tromp-Taylor area score: stones of `color` + empty points enclosed only by `color`.
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
                let mut region_size = 0i32;
                let mut stack = vec![(r, c)];
                let mut borders_me = false;
                let mut borders_other = false;
                visited[r][c] = true;
                while let Some((cr, cc)) = stack.pop() {
                    region_size += 1;
                    for (nr, nc) in board.neighbors(cr, cc) {
                        match board.get(nr, nc) {
                            Color::Empty => {
                                if !visited[nr][nc] {
                                    visited[nr][nc] = true;
                                    stack.push((nr, nc));
                                }
                            }
                            other if other == color => borders_me = true,
                            _ => borders_other = true,
                        }
                    }
                }
                if borders_me && !borders_other { score += region_size; }
            }
        }
    }
    score
}

// ── Tactical helpers ──

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

fn is_self_atari(game: &Game, row: usize, col: usize) -> bool {
    let mut b = game.board().clone();
    let me = game.current_player_color();
    b.set(row, col, me);
    b.remove_captured(me.opposite());
    b.count_liberties(row, col) <= 1
}

fn is_eye_for(board: &Board, row: usize, col: usize, color: Color) -> bool {
    if board.get(row, col) != Color::Empty { return false; }
    board.neighbors(row, col).iter().all(|&(nr, nc)| board.get(nr, nc) == color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eye_detection_center() {
        let mut b = Board::new(9);
        b.set(3, 4, Color::Black);
        b.set(5, 4, Color::Black);
        b.set(4, 3, Color::Black);
        b.set(4, 5, Color::Black);
        assert!(is_eye_for(&b, 4, 4, Color::Black));
        assert!(!is_eye_for(&b, 4, 4, Color::White));
    }

    #[test]
    fn area_score_separates_territory() {
        // On a 5x5 board, a black wall at col 1 and a white wall at col 3
        // divide the board into three regions:
        //   col 0  → bordered only by black  → 5 black territory points
        //   col 2  → bordered by both        → neutral (0 for either)
        //   col 4  → bordered only by white  → 5 white territory points
        let mut b = Board::new(5);
        for r in 0..5 {
            b.set(r, 1, Color::Black);
            b.set(r, 3, Color::White);
        }
        // Black: 5 stones + 5 territory = 10
        assert_eq!(area_score(&b, Color::Black), 10);
        // White: 5 stones + 5 territory = 10
        assert_eq!(area_score(&b, Color::White), 10);
    }

    #[test]
    fn area_score_neutral_region_counts_for_neither() {
        // A single black stone and a single white stone on opposite corners
        // of a 5x5: neither side encloses any territory fully.
        let mut b = Board::new(5);
        b.set(0, 0, Color::Black);
        b.set(4, 4, Color::White);
        // (0,0) borders the large empty region which also borders white → neutral.
        assert_eq!(area_score(&b, Color::Black), 1); // just the stone itself
        assert_eq!(area_score(&b, Color::White), 1);
    }

    #[test]
    fn dan_returns_legal_move_on_empty_board() {
        let g = Game::new(9);
        let mv = get_move(&g, 2, 42);
        assert!(mv >= 0 && mv < 81);
    }

    #[test]
    fn dan_takes_free_capture() {
        // White stone in corner with one liberty; Dan must take it.
        let mut g = Game::new(9);
        g.place_stone(0, 1); // Black
        g.place_stone(0, 0); // White — corner, one liberty left at (1,0)
        let mv = get_move(&g, 2, 7);
        assert_eq!(mv, (1 * 9 + 0) as i32);
    }

    #[test]
    fn average_takes_free_capture() {
        // Average always takes a capture when one is available (deterministic).
        let mut g = Game::new(9);
        g.place_stone(0, 1); // Black
        g.place_stone(0, 0); // White — corner, one liberty left at (1,0)
        let mv = get_move(&g, 1, 0); // difficulty=1 (average), seed irrelevant
        assert_eq!(mv, (1 * 9 + 0) as i32);
    }
}
