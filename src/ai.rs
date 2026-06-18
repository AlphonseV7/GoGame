use crate::game::Game;
use crate::board::{Board, Color};

/// Komi used when judging playout outcomes (matches Game's Japanese komi).
const KOMI: f64 = 6.5;

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
    /// Weighted random pick: each candidate's probability ∝ its score.
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
        0 => { let (r, c) = noob(game, &moves, &mut rng); (r * size + c) as i32 }
        1 => { let (r, c) = intermediate(game, &moves, &mut rng); (r * size + c) as i32 }
        _ => dan_index(game, &moves, &mut rng),
    }
}

// ── Difficulty 0 — Beginner ───────────────────────────────────────────────────
//
// Still random but avoids the two moves that make a bot look broken:
// filling its own eyes and self-atari.  Also always takes a free capture.

fn noob(game: &Game, moves: &[(usize, usize)], rng: &mut Rng) -> (usize, usize) {
    // Always capture if possible.
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
// Scores every legal move with move_prior() and picks proportionally to that
// score (weighted random).  This gives tactical awareness and opening sense
// without the cost of MCTS.

fn intermediate(game: &Game, moves: &[(usize, usize)], rng: &mut Rng) -> (usize, usize) {
    let board = game.board();
    let me = game.current_player_color();
    // Filter out eyes and self-ataris first.
    let cands: Vec<_> = moves.iter()
        .filter(|&&(r, c)| !is_eye_for(board, r, c, me) && !is_self_atari(game, r, c))
        .copied().collect();
    let pool = if cands.is_empty() { moves.to_vec() } else { cands };
    let scores: Vec<f64> = pool.iter().map(|&(r, c)| move_prior(game, r, c)).collect();
    rng.weighted_pick(&pool, &scores)
}

// ── Difficulty 2 — Dan (MCTS + priors) ───────────────────────────────────────

fn dan_index(game: &Game, moves: &[(usize, usize)], rng: &mut Rng) -> i32 {
    // Shortcut 1: take a free capture immediately.
    for &(r, c) in moves {
        if captures_opponent(game, r, c) && !is_self_atari(game, r, c) {
            return (r * game.board_size() + c) as i32;
        }
    }
    // Shortcut 2: save own group that is in atari (1 liberty left).
    for &(r, c) in moves {
        if saves_critical_group(game, r, c) && !is_self_atari(game, r, c) {
            return (r * game.board_size() + c) as i32;
        }
    }

    let size = game.board_size();
    // More iterations than before because focused expansion makes each
    // iteration count for much more.
    let iterations = match size {
        n if n >= 19 => 300,
        n if n >= 13 => 800,
        _            => 1800,
    };
    mcts(game, rng, iterations)
}

// ── MCTS (UCT + move priors) ──────────────────────────────────────────────────

struct Node {
    game: Game,
    mv: Option<(usize, usize)>,
    parent: Option<usize>,
    children: Vec<usize>,
    /// Remaining untried moves, pre-sorted best-last (we pop from the back).
    untried: Vec<(usize, usize)>,
    wins: f64,
    visits: f64,
    player_just_moved: Color,
    to_move: Color,
}

fn mcts(game: &Game, rng: &mut Rng, iterations: usize) -> i32 {
    const C: f64 = 1.2; // slightly lower exploration bias than sqrt(2); prioritise exploitation

    let root_cands = scored_candidates(game);
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
        // 1. Selection — descend fully-expanded nodes by UCT.
        let mut idx = 0;
        while nodes[idx].untried.is_empty() && !nodes[idx].children.is_empty() {
            let parent_visits = nodes[idx].visits;
            let best_child = *nodes[idx].children.iter().max_by(|&&a, &&b| {
                let uct = |i: usize| {
                    let n = &nodes[i];
                    n.wins / n.visits + C * (parent_visits.ln() / n.visits).sqrt()
                };
                uct(a).partial_cmp(&uct(b)).unwrap()
            }).unwrap();
            idx = best_child;
        }

        // 2. Expansion — try the best untried move (sorted best-last, pop).
        if !nodes[idx].untried.is_empty() {
            let mv = nodes[idx].untried.pop().unwrap();
            let mover = nodes[idx].to_move;
            let mut child_game = nodes[idx].game.clone();
            child_game.place_stone(mv.0, mv.1);
            let child_cands = scored_candidates(&child_game);
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

        // 3. Simulation — guided playout.
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
    let best = nodes[0].children.iter().max_by(|&&a, &&b| {
        nodes[a].visits.partial_cmp(&nodes[b].visits).unwrap()
    });
    match best.and_then(|&ci| nodes[ci].mv) {
        Some((r, c)) => (r * game.board_size() + c) as i32,
        None => -1,
    }
}

// ── Move candidates and scoring ───────────────────────────────────────────────

/// Legal moves minus obvious blunders, sorted ASCENDING by prior (best pops off the back).
fn scored_candidates(game: &Game) -> Vec<(usize, usize)> {
    let board = game.board();
    let me = game.current_player_color();
    let mut cands: Vec<(f64, (usize, usize))> = game.get_legal_moves()
        .into_iter()
        .filter(|&(r, c)| !is_self_atari(game, r, c) && !is_eye_for(board, r, c, me))
        .map(|(r, c)| (move_prior(game, r, c), (r, c)))
        .collect();
    // Sort ascending; pop() gives highest-prior move.
    cands.sort_unstable_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    cands.into_iter().map(|(_, mv)| mv).collect()
}

/// Comprehensive heuristic score for playing at (row, col).
/// Higher = more desirable.  Used as MCTS expansion priority and
/// as the policy weight in simulation and intermediate AI.
fn move_prior(game: &Game, row: usize, col: usize) -> f64 {
    let board = game.board();
    let me = game.current_player_color();
    let opp = me.opposite();
    let size = board.size();

    // ── Tactical urgency (checked in order of priority) ──────────────────
    if captures_opponent(game, row, col) { return 10_000.0; }
    if saves_critical_group(game, row, col) { return 8_000.0; }

    let mut score = 1.0f64;

    // Immediate threat: after our move, do opponent groups drop to 1 liberty?
    {
        let mut b = board.clone();
        b.set(row, col, me);
        b.remove_captured(opp);
        for (nr, nc) in board.neighbors(row, col) {
            if board.get(nr, nc) == opp {
                let libs = b.count_liberties(nr, nc);
                if libs == 1 { score += 4_000.0; } // will be capturable next turn
                else if libs == 2 { score += 800.0; }
                else if libs == 3 { score += 150.0; }
            }
        }
    }

    // Rescue own groups approaching danger (2–3 liberties).
    for (nr, nc) in board.neighbors(row, col) {
        if board.get(nr, nc) == me {
            let libs = board.count_liberties(nr, nc);
            if libs == 2 { score += 1_500.0; }
            else if libs == 3 { score += 300.0; }
        }
    }

    // Reduce opponent groups with few liberties.
    for (nr, nc) in board.neighbors(row, col) {
        if board.get(nr, nc) == opp {
            let libs = board.count_liberties(nr, nc);
            if libs == 2 { score += 600.0; }
            else if libs == 3 { score += 100.0; }
        }
    }

    // ── Connectivity ─────────────────────────────────────────────────────
    // Bonus for connecting two separate own groups (bridging).
    let own_neighbor_groups: Vec<_> = board.neighbors(row, col).into_iter()
        .filter(|&(nr, nc)| board.get(nr, nc) == me)
        .collect();
    if own_neighbor_groups.len() >= 2 {
        // Check they are distinct groups (simplification: different positions).
        let g0 = board.get_group(own_neighbor_groups[0].0, own_neighbor_groups[0].1);
        let g1 = board.get_group(own_neighbor_groups[1].0, own_neighbor_groups[1].1);
        if !g0.contains(&own_neighbor_groups[1]) { score += 400.0; }
        let _ = g1; // silence warning
    }

    // ── Opening principles ────────────────────────────────────────────────
    let total_stones = board.count_stones(me) + board.count_stones(opp);
    let opening_phase = total_stones < size * 4;
    if opening_phase {
        score += opening_bonus(row, col, size);
    }

    // ── Positional value ──────────────────────────────────────────────────
    // Contact play bonus: playing next to any stone is usually relevant.
    let has_neighbor = board.neighbors(row, col).iter()
        .any(|&(nr, nc)| board.get(nr, nc) != Color::Empty);
    if has_neighbor { score += 80.0; }

    // Diagonal proximity bonus (still near the action but not contact).
    let diag_neighbor = diagonal_neighbors(row, col, size).iter()
        .any(|&(dr, dc)| board.get(dr, dc) != Color::Empty);
    if diag_neighbor && !has_neighbor { score += 25.0; }

    // Midgame: slight bonus for third/fourth line (good for territory).
    if !opening_phase {
        let edge_r = row.min(size.saturating_sub(1 + row));
        let edge_c = col.min(size.saturating_sub(1 + col));
        let line = edge_r.min(edge_c);
        if line == 2 || line == 3 { score += 20.0; }
    }

    score
}

/// Returns a score bonus for openings: third/fourth line corners and sides.
fn opening_bonus(r: usize, c: usize, size: usize) -> f64 {
    // Distance to nearest edge and corner.
    let edge_r = r.min(size.saturating_sub(1 + r));
    let edge_c = c.min(size.saturating_sub(1 + c));
    let corner_manhattan = edge_r + edge_c; // 0 = corner, increasing toward centre

    // 3-3, 4-4, 3-4 (hoshi/komoku) zone: corner_manhattan 4..=6
    // Side approach / shimari extension: corner_manhattan 7..=9
    // Centre (tengen/moyo): edge_r and edge_c both large
    match corner_manhattan {
        4 | 5 => 500.0, // classic corner points (3-4, 4-4, 3-3)
        6 | 7 => 300.0, // extensions / approach moves
        8 | 9 => 150.0, // wider extensions
        _ => {
            // Central area: good for moyo in midgame opening
            let center_r = (size / 2) as isize - r as isize;
            let center_c = (size / 2) as isize - c as isize;
            let center_dist = (center_r.abs() + center_c.abs()) as usize;
            if center_dist <= 2 { 200.0 } else { 30.0 }
        }
    }
}

fn diagonal_neighbors(row: usize, col: usize, size: usize) -> Vec<(usize, usize)> {
    let mut v = Vec::new();
    if row > 0 && col > 0            { v.push((row-1, col-1)); }
    if row > 0 && col + 1 < size     { v.push((row-1, col+1)); }
    if row + 1 < size && col > 0     { v.push((row+1, col-1)); }
    if row + 1 < size && col + 1 < size { v.push((row+1, col+1)); }
    v
}

// ── Simulation ────────────────────────────────────────────────────────────────

/// Guided playout. Always takes free captures; otherwise weighted-random.
fn simulate(start: &Game, rng: &mut Rng) -> Color {
    let mut sim = start.clone();
    let size = sim.board_size();
    let max_moves = size * size * 2;
    let mut played = 0;

    while !sim.is_game_over() && played < max_moves {
        let player = sim.current_player_color();

        // 1. Always grab a free capture if one exists.
        if let Some((cr, cc)) = find_capture(sim.board(), player) {
            if !is_self_atari(&sim, cr, cc) {
                sim.place_stone(cr, cc);
                played += 1;
                continue;
            }
        }

        // 2. Weighted random from non-eye non-self-atari moves.
        let mut placed = false;
        for _ in 0..10 {
            let r = (rng.next() as usize) % size;
            let c = (rng.next() as usize) % size;
            let board = sim.board();
            if board.get(r, c) != Color::Empty { continue; }
            if is_eye_for(board, r, c, player) { continue; }
            let open = board.neighbors(r, c).iter()
                .filter(|&&(nr, nc)| board.get(nr, nc) == Color::Empty).count();
            if open < 2 && is_self_atari(&sim, r, c) { continue; }
            if sim.place_stone(r, c) { placed = true; break; }
        }
        if !placed { sim.pass_turn(); }
        played += 1;
    }

    let b = area_score(sim.board(), Color::Black) as f64;
    let w = area_score(sim.board(), Color::White) as f64 + KOMI;
    if b > w { Color::Black } else if w > b { Color::White } else { Color::Empty }
}

/// Scan for an opponent group with exactly 1 liberty and return that liberty.
fn find_capture(board: &Board, color: Color) -> Option<(usize, usize)> {
    let opp = color.opposite();
    let size = board.size();
    let mut checked = vec![false; size * size];
    for r in 0..size {
        for c in 0..size {
            let i = r * size + c;
            if checked[i] || board.get(r, c) != opp { continue; }
            let group = board.get_group(r, c);
            for &(gr, gc) in &group { checked[gr * size + gc] = true; }
            let mut lib: Option<(usize, usize)> = None;
            let mut lib_count = 0;
            'outer: for &(gr, gc) in &group {
                for (nr, nc) in board.neighbors(gr, gc) {
                    if board.get(nr, nc) == Color::Empty {
                        lib_count += 1;
                        lib = Some((nr, nc));
                        if lib_count > 1 { break 'outer; }
                    }
                }
            }
            if lib_count == 1 { return lib; }
        }
    }
    None
}

// ── Area score (Tromp-Taylor) ─────────────────────────────────────────────────

fn area_score(board: &Board, color: Color) -> i32 {
    let size = board.size();
    let mut visited = vec![false; size * size];
    let mut score = 0i32;

    for r in 0..size {
        for c in 0..size {
            let cell = board.get(r, c);
            if cell == color {
                score += 1;
            } else if cell == Color::Empty && !visited[r * size + c] {
                let mut region_size = 0i32;
                let mut stack = vec![(r, c)];
                let mut borders_me = false;
                let mut borders_other = false;
                visited[r * size + c] = true;
                while let Some((cr, cc)) = stack.pop() {
                    region_size += 1;
                    for (nr, nc) in board.neighbors(cr, cc) {
                        match board.get(nr, nc) {
                            Color::Empty => {
                                if !visited[nr * size + nc] {
                                    visited[nr * size + nc] = true;
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

// ── Tactical helpers ──────────────────────────────────────────────────────────

fn captures_opponent(game: &Game, row: usize, col: usize) -> bool {
    let board = game.board();
    let opp = game.current_player_color().opposite();
    board.neighbors(row, col).iter().any(|&(nr, nc)| {
        board.get(nr, nc) == opp && board.count_liberties(nr, nc) == 1
    })
}

/// Returns true if playing here saves one of our own groups that is in atari
/// AND that group has more than 1 stone (saving a single stone is lower priority).
fn saves_critical_group(game: &Game, row: usize, col: usize) -> bool {
    let board = game.board();
    let me = game.current_player_color();
    board.neighbors(row, col).iter().any(|&(nr, nc)| {
        if board.get(nr, nc) != me { return false; }
        if board.count_liberties(nr, nc) != 1 { return false; }
        board.get_group(nr, nc).len() > 1
    })
}

fn saves_atari(game: &Game, row: usize, col: usize) -> bool {
    let board = game.board();
    let cur = game.current_player_color();
    board.neighbors(row, col).iter().any(|&(nr, nc)| {
        board.get(nr, nc) == cur && board.count_liberties(nr, nc) == 1
    })
}

fn ataris_opponent(game: &Game, row: usize, col: usize) -> bool {
    let me = game.current_player_color();
    let opp = me.opposite();
    let mut b = game.board().clone();
    b.set(row, col, me);
    b.neighbors(row, col).iter().any(|&(nr, nc)| {
        b.get(nr, nc) == opp && b.count_liberties(nr, nc) == 1
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
    fn area_score_separates_territory() {
        let mut b = Board::new(5);
        for r in 0..5 {
            b.set(r, 1, Color::Black);
            b.set(r, 3, Color::White);
        }
        assert_eq!(area_score(&b, Color::Black), 10);
        assert_eq!(area_score(&b, Color::White), 10);
    }

    #[test]
    fn area_score_neutral_region_counts_for_neither() {
        let mut b = Board::new(5);
        b.set(0, 0, Color::Black);
        b.set(4, 4, Color::White);
        assert_eq!(area_score(&b, Color::Black), 1);
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
        let mut g = Game::new(9);
        g.place_stone(0, 1); // Black
        g.place_stone(0, 0); // White — corner, one liberty left at (1,0)
        let mv = get_move(&g, 2, 7);
        assert_eq!(mv, (1 * 9 + 0) as i32);
    }

    #[test]
    fn intermediate_takes_free_capture() {
        let mut g = Game::new(9);
        g.place_stone(0, 1); // Black
        g.place_stone(0, 0); // White — corner, one liberty left at (1,0)
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
    fn find_capture_finds_lone_stone() {
        let mut b = Board::new(9);
        // White stone at (0,0) with liberties only at (0,1) and (1,0).
        // Surround it so only (1,0) remains.
        b.set(0, 1, Color::Black);
        b.set(0, 0, Color::White);
        // (1,0) is the last liberty.
        let cap = find_capture(&b, Color::Black);
        assert_eq!(cap, Some((1, 0)));
    }

    #[test]
    fn move_prior_rates_capture_highest() {
        let mut g = Game::new(9);
        g.place_stone(0, 1); // Black
        g.place_stone(0, 0); // White in corner
        // (1,0) captures White; any other move should score lower.
        let cap_score = move_prior(&g, 1, 0);
        let other_score = move_prior(&g, 4, 4);
        assert!(cap_score > other_score);
    }

    #[test]
    fn opening_bonus_prefers_corner_region() {
        // On a 19x19 the hoshi/komoku zone should score higher than the edge.
        let edge   = opening_bonus(0, 0, 19); // corner itself (legal move but unusual)
        let hoshi  = opening_bonus(3, 3, 19); // classic 4-4 point
        let center = opening_bonus(9, 9, 19); // tengen
        // hoshi beats raw corner cell, and centre also beats edge.
        assert!(hoshi > edge);
        assert!(center > edge);
    }

    #[test]
    fn intermediate_returns_legal_move() {
        let g = Game::new(9);
        let mv = get_move(&g, 1, 99);
        assert!(mv >= 0 && mv < 81);
    }
}
