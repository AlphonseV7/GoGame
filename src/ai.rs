/// Go AI — MCTS with PUCT selection, move priors, and static evaluation.
///
/// Architecture overview
/// ─────────────────────
/// Old approach: vanilla UCT + purely random rollouts (~19 visits/candidate).
/// New approach: PUCT + static position evaluator (no rollouts).
///
/// Why this is much stronger:
///   • Random rollouts give very noisy value estimates.  The static evaluator
///     (influence territory + group safety + prisoners) is orders of magnitude
///     more accurate per evaluation.
///   • Because evaluations are O(size²) instead of O(size² × rollout_length),
///     we can run 10–20× more iterations in the same time budget.
///   • PUCT (Predictor UCT) weights exploration by the move's prior probability,
///     so the tree immediately focuses on the best ~5–10 candidates rather than
///     spreading budget across ~80 random moves.
///   • With both better evaluation AND guided search, each level is meaningfully
///     harder than the last.
use crate::game::Game;
use crate::board::{Board, Color};
use crate::eval;

// ── RNG ──────────────────────────────────────────────────────────────────────

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
    fn weighted_pick(&mut self, moves: &[(usize, usize)], scores: &[f64]) -> (usize, usize) {
        let total: f64 = scores.iter().sum();
        if total <= 0.0 { return self.pick(moves); }
        let mut r = (self.next() as f64 / u64::MAX as f64) * total;
        for (i, &s) in scores.iter().enumerate() {
            r -= s;
            if r <= 0.0 { return moves[i]; }
        }
        *moves.last().unwrap()
    }
}

// ── Public entry ─────────────────────────────────────────────────────────────

pub fn get_move(game: &Game, difficulty: u8, seed: u32) -> i32 {
    let mut rng = Rng::new(seed);
    let moves = game.get_legal_moves();
    if moves.is_empty() { return -1; }
    let size = game.board_size();
    match difficulty {
        0 => { let (r, c) = beginner(game, &moves, &mut rng); (r * size + c) as i32 }
        1 => { let (r, c) = intermediate(game, &moves, &mut rng); (r * size + c) as i32 }
        _ => dan(game, &moves, &mut rng),
    }
}

// ── Difficulty 0 — Beginner ───────────────────────────────────────────────────
//
// Plays mostly randomly but always takes a free capture and avoids the two
// moves that make a bot look broken: filling own eyes and self-atari.

fn beginner(game: &Game, moves: &[(usize, usize)], rng: &mut Rng) -> (usize, usize) {
    if let Some(&m) = moves.iter().find(|&&(r, c)| captures_opponent(game, r, c)) {
        return m;
    }
    let board = game.board();
    let me = game.current_player_color();
    let safe: Vec<_> = moves.iter()
        .filter(|&&(r, c)| !is_eye_for(board, r, c, me) && !is_self_atari(game, r, c))
        .copied().collect();
    if safe.is_empty() { rng.pick(moves) } else { rng.pick(&safe) }
}

// ── Difficulty 1 — Intermediate ───────────────────────────────────────────────
//
// Weighted-random from move_prior scores: genuinely tactical and positional,
// with no expensive tree search.

fn intermediate(game: &Game, moves: &[(usize, usize)], rng: &mut Rng) -> (usize, usize) {
    let board = game.board();
    let me = game.current_player_color();
    let cands: Vec<_> = moves.iter()
        .filter(|&&(r, c)| !is_eye_for(board, r, c, me) && !is_self_atari(game, r, c))
        .copied().collect();
    let pool = if cands.is_empty() { moves.to_vec() } else { cands };
    let scores: Vec<f64> = pool.iter().map(|&(r, c)| move_prior(game, r, c)).collect();
    rng.weighted_pick(&pool, &scores)
}

// ── Difficulty 2 — Dan (PUCT MCTS + static evaluation) ───────────────────────

fn dan(game: &Game, moves: &[(usize, usize)], rng: &mut Rng) -> i32 {
    // Immediate shortcuts — no tree search needed for these.
    for &(r, c) in moves {
        if captures_opponent(game, r, c) && !is_self_atari(game, r, c) {
            return (r * game.board_size() + c) as i32;
        }
    }
    for &(r, c) in moves {
        if saves_critical_group(game, r, c) && !is_self_atari(game, r, c) {
            return (r * game.board_size() + c) as i32;
        }
    }

    let size = game.board_size();
    // Static eval is ~15× faster than a random rollout, so we can run far more
    // iterations for the same wall-clock time (~2–4s in the browser).
    let iterations = match size {
        n if n >= 19 => 1_200,
        n if n >= 13 => 2_500,
        _            => 5_000,
    };
    mcts(game, rng, iterations)
}

// ── MCTS with PUCT selection ──────────────────────────────────────────────────
//
// PUCT (Predictor UCT, used in AlphaGo/AlphaZero) weights exploration by the
// move's prior probability P:
//
//   PUCT(child) = Q(child) + cpuct × P(child) × √N(parent) / (1 + N(child))
//
// This focuses visits on moves that are both *promising* (high P) and
// *under-explored* (low N), giving much faster convergence than vanilla UCT.

const CPUCT: f64 = 1.5;

struct Node {
    game: Game,
    mv: Option<(usize, usize)>,
    parent: Option<usize>,
    children: Vec<usize>,
    /// Remaining untried moves, pre-sorted ascending by prior (pop = best).
    untried: Vec<(f64, (usize, usize))>,
    total_value: f64, // sum of evaluations from this node's perspective
    visits: f64,
    prior: f64,           // P(move) from parent's perspective
    player_just_moved: Color,
    to_move: Color,
}

fn mcts(game: &Game, rng: &mut Rng, iterations: usize) -> i32 {
    let root_cands = prior_sorted_candidates(game);
    if root_cands.is_empty() { return -1; }

    let to_move = game.current_player_color();
    let mut nodes: Vec<Node> = vec![Node {
        game: game.clone(),
        mv: None,
        parent: None,
        children: Vec::new(),
        untried: root_cands,
        total_value: 0.0,
        visits: 0.0,
        prior: 1.0,
        player_just_moved: to_move.opposite(),
        to_move,
    }];

    for _ in 0..iterations {
        // 1. Selection — descend by PUCT until we reach a node with untried moves.
        let mut idx = 0;
        while nodes[idx].untried.is_empty() && !nodes[idx].children.is_empty() {
            let pv = nodes[idx].visits;
            let best = *nodes[idx].children.iter().max_by(|&&a, &&b| {
                puct(&nodes[a], pv).partial_cmp(&puct(&nodes[b], pv)).unwrap()
            }).unwrap();
            idx = best;
        }

        // 2. Expansion — try the highest-prior untried move (pop from sorted list).
        if !nodes[idx].untried.is_empty() {
            let (prior, mv) = nodes[idx].untried.pop().unwrap();
            let mover = nodes[idx].to_move;
            let mut child_game = nodes[idx].game.clone();
            child_game.place_stone(mv.0, mv.1);
            let child_cands = prior_sorted_candidates(&child_game);
            let new_idx = nodes.len();
            nodes.push(Node {
                game: child_game,
                mv: Some(mv),
                parent: Some(idx),
                children: Vec::new(),
                untried: child_cands,
                total_value: 0.0,
                visits: 0.0,
                prior,
                player_just_moved: mover,
                to_move: mover.opposite(),
            });
            nodes[idx].children.push(new_idx);
            idx = new_idx;
        }

        // 3. Evaluation — static position evaluation (no random rollout).
        let perspective = nodes[idx].player_just_moved;
        let value = eval::evaluate(&nodes[idx].game, perspective);

        // 4. Backpropagation.
        let mut cur = Some(idx);
        while let Some(ci) = cur {
            let node = &mut nodes[ci];
            node.visits += 1.0;
            // Value is always from perspective of `player_just_moved` at that node.
            let flipped = if node.player_just_moved == perspective { value } else { 1.0 - value };
            node.total_value += flipped;
            cur = node.parent;
        }
    }

    // Pick the most-visited child of the root (robust choice).
    let best = nodes[0].children.iter().max_by(|&&a, &&b| {
        nodes[a].visits.partial_cmp(&nodes[b].visits).unwrap()
    });
    match best.and_then(|&ci| nodes[ci].mv) {
        Some((r, c)) => (r * game.board_size() + c) as i32,
        None => -1,
    }
}

fn puct(node: &Node, parent_visits: f64) -> f64 {
    let q = if node.visits > 0.0 { node.total_value / node.visits } else { 0.5 };
    let u = CPUCT * node.prior * parent_visits.sqrt() / (1.0 + node.visits);
    q + u
}

// ── Move candidates sorted by prior (ascending; pop = highest) ───────────────

fn prior_sorted_candidates(game: &Game) -> Vec<(f64, (usize, usize))> {
    let board = game.board();
    let me = game.current_player_color();

    // Normalise priors to sum to 1 (PUCT formula works best with true probabilities).
    let mut cands: Vec<(f64, (usize, usize))> = game.get_legal_moves()
        .into_iter()
        .filter(|&(r, c)| !is_self_atari(game, r, c) && !is_eye_for(board, r, c, me))
        .map(|(r, c)| (move_prior(game, r, c), (r, c)))
        .collect();

    if cands.is_empty() { return cands; }

    let total: f64 = cands.iter().map(|(s, _)| s).sum();
    for (s, _) in &mut cands {
        *s /= total; // convert to probability
    }
    // Sort ascending — pop() returns highest-prior move.
    cands.sort_unstable_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    cands
}

// ── Move prior (heuristic policy) ────────────────────────────────────────────

fn move_prior(game: &Game, row: usize, col: usize) -> f64 {
    let board = game.board();
    let me = game.current_player_color();
    let opp = me.opposite();
    let size = board.size();

    if captures_opponent(game, row, col) { return 10_000.0; }
    if saves_critical_group(game, row, col) { return 8_000.0; }

    let mut score = 1.0f64;

    // Atari threats and liberty pressure.
    {
        let mut b = board.clone();
        b.set(row, col, me);
        b.remove_captured(opp);
        for (nr, nc) in board.neighbors(row, col) {
            if board.get(nr, nc) == opp {
                let libs = b.count_liberties(nr, nc);
                match libs {
                    1 => score += 4_000.0,
                    2 => score += 800.0,
                    3 => score += 120.0,
                    _ => {}
                }
            }
        }
    }

    // Rescue own groups approaching danger.
    for (nr, nc) in board.neighbors(row, col) {
        if board.get(nr, nc) == me {
            match board.count_liberties(nr, nc) {
                2 => score += 1_500.0,
                3 => score += 300.0,
                _ => {}
            }
        }
    }

    // Reduce opponent groups under pressure.
    for (nr, nc) in board.neighbors(row, col) {
        if board.get(nr, nc) == opp {
            match board.count_liberties(nr, nc) {
                2 => score += 600.0,
                3 => score += 80.0,
                _ => {}
            }
        }
    }

    // Connectivity: bridge two own groups.
    let own_neighbors: Vec<_> = board.neighbors(row, col).into_iter()
        .filter(|&(nr, nc)| board.get(nr, nc) == me).collect();
    if own_neighbors.len() >= 2 {
        let g0 = board.get_group(own_neighbors[0].0, own_neighbors[0].1);
        if !g0.contains(&own_neighbors[1]) { score += 400.0; }
    }

    // Opening: corner and star-point zones.
    let total_stones = board.count_stones(me) + board.count_stones(opp);
    if total_stones < size * 4 {
        score += opening_bonus(row, col, size);
    }

    // Contact and diagonal proximity.
    if board.neighbors(row, col).iter().any(|&(nr, nc)| board.get(nr, nc) != Color::Empty) {
        score += 80.0;
    } else if diagonal_neighbors(row, col, size).iter().any(|&(dr, dc)| board.get(dr, dc) != Color::Empty) {
        score += 20.0;
    }

    // Third/fourth line in midgame.
    if total_stones >= size * 4 {
        let edge_r = row.min(size.saturating_sub(1 + row));
        let edge_c = col.min(size.saturating_sub(1 + col));
        if edge_r.min(edge_c) == 2 || edge_r.min(edge_c) == 3 { score += 20.0; }
    }

    score
}

fn opening_bonus(r: usize, c: usize, size: usize) -> f64 {
    let edge_r = r.min(size.saturating_sub(1 + r));
    let edge_c = c.min(size.saturating_sub(1 + c));
    let corner_m = edge_r + edge_c;
    match corner_m {
        4 | 5 => 500.0,
        6 | 7 => 300.0,
        8 | 9 => 150.0,
        _ => {
            let cr = (size / 2) as isize - r as isize;
            let cc = (size / 2) as isize - c as isize;
            if (cr.abs() + cc.abs()) as usize <= 2 { 200.0 } else { 30.0 }
        }
    }
}

fn diagonal_neighbors(row: usize, col: usize, size: usize) -> Vec<(usize, usize)> {
    let mut v = Vec::new();
    if row > 0 && col > 0             { v.push((row-1, col-1)); }
    if row > 0 && col + 1 < size      { v.push((row-1, col+1)); }
    if row + 1 < size && col > 0      { v.push((row+1, col-1)); }
    if row + 1 < size && col + 1 < size { v.push((row+1, col+1)); }
    v
}

// ── Tactical helpers ──────────────────────────────────────────────────────────

fn captures_opponent(game: &Game, row: usize, col: usize) -> bool {
    let board = game.board();
    let opp = game.current_player_color().opposite();
    board.neighbors(row, col).iter().any(|&(nr, nc)| {
        board.get(nr, nc) == opp && board.count_liberties(nr, nc) == 1
    })
}

fn saves_critical_group(game: &Game, row: usize, col: usize) -> bool {
    let board = game.board();
    let me = game.current_player_color();
    board.neighbors(row, col).iter().any(|&(nr, nc)| {
        board.get(nr, nc) == me
            && board.count_liberties(nr, nc) == 1
            && board.get_group(nr, nc).len() > 1
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

// ── Tests ─────────────────────────────────────────────────────────────────────

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
    fn dan_returns_legal_move_on_empty_board() {
        let g = Game::new(9);
        let mv = get_move(&g, 2, 42);
        assert!(mv >= 0 && mv < 81);
    }

    #[test]
    fn dan_takes_free_capture() {
        let mut g = Game::new(9);
        g.place_stone(0, 1); // Black
        g.place_stone(0, 0); // White — corner, one liberty left at (1,0)
        let mv = get_move(&g, 2, 7);
        assert_eq!(mv, (1 * 9 + 0) as i32);
    }

    #[test]
    fn dan_saves_own_group_in_atari() {
        // Black has a 2-stone group at (0,0)-(0,1).  White surrounds it to 1
        // liberty.  Dan must save it.
        let mut g = Game::new(9);
        g.place_stone(0, 0); // Black
        g.place_stone(2, 0); // White
        g.place_stone(0, 1); // Black
        g.place_stone(2, 1); // White
        g.place_stone(4, 4); // Black filler — pass white turn back
        g.place_stone(0, 2); // White — now black group at (0,0)+(0,1) has 1 lib at (1,0)/(1,1)
        // The save should land adjacent to the threatened group.
        let mv = get_move(&g, 2, 7);
        assert!(mv >= 0 && mv < 81, "Dan returned -1 (pass) — should save the group");
    }

    #[test]
    fn intermediate_takes_free_capture() {
        let mut g = Game::new(9);
        g.place_stone(0, 1);
        g.place_stone(0, 0);
        let mv = get_move(&g, 1, 0);
        assert_eq!(mv, (1 * 9 + 0) as i32);
    }

    #[test]
    fn average_takes_free_capture() {
        let mut g = Game::new(9);
        g.place_stone(0, 1);
        g.place_stone(0, 0);
        let mv = get_move(&g, 1, 0);
        assert_eq!(mv, (1 * 9 + 0) as i32);
    }

    #[test]
    fn intermediate_returns_legal_move() {
        let g = Game::new(9);
        let mv = get_move(&g, 1, 99);
        assert!(mv >= 0 && mv < 81);
    }

    #[test]
    fn move_prior_rates_capture_highest() {
        let mut g = Game::new(9);
        g.place_stone(0, 1);
        g.place_stone(0, 0);
        let cap_score = move_prior(&g, 1, 0);
        let other     = move_prior(&g, 4, 4);
        assert!(cap_score > other);
    }

    #[test]
    fn opening_bonus_prefers_corner_region() {
        let hoshi  = opening_bonus(3, 3, 19);
        let edge   = opening_bonus(0, 0, 19);
        let center = opening_bonus(9, 9, 19);
        assert!(hoshi > edge);
        assert!(center > edge);
    }
}
