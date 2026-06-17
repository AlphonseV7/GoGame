use crate::game::Game;
use crate::board::Color;

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

fn noob(moves: &[(usize, usize)], rng: &mut Rng) -> (usize, usize) {
    rng.pick(moves)
}

fn average(game: &Game, moves: &[(usize, usize)], rng: &mut Rng) -> (usize, usize) {
    if let Some(&m) = moves.iter().find(|&&(r,c)| captures_opponent(game,r,c)) { return m; }
    if let Some(&m) = moves.iter().find(|&&(r,c)| saves_atari(game,r,c)) { return m; }
    let board = game.board();
    let near: Vec<_> = moves.iter().filter(|&&(r,c)| {
        board.neighbors(r,c).iter().any(|&(nr,nc)| board.get(nr,nc) != Color::Empty)
    }).copied().collect();
    if !near.is_empty() { rng.pick(&near) } else { noob(moves, rng) }
}

fn dan(game: &Game, moves: &[(usize, usize)], rng: &mut Rng) -> (usize, usize) {
    let size = game.board_size();
    let player = game.current_player_color();
    let max_cands = if size >= 19 { 35 } else if size >= 13 { 30 } else { moves.len().min(50) };
    let sims      = if size >= 19 { 6  } else if size >= 13 { 10 } else { 18 };

    let mut priority: Vec<(usize,usize)> = moves.iter()
        .filter(|&&(r,c)| captures_opponent(game,r,c) || saves_atari(game,r,c))
        .copied().collect();
    let board = game.board();
    let mut near: Vec<(usize,usize)> = moves.iter().filter(|&&(r,c)| {
        !priority.contains(&(r,c)) &&
        board.neighbors(r,c).iter().any(|&(nr,nc)| board.get(nr,nc) != Color::Empty)
    }).copied().collect();
    rng.shuffle(&mut near);
    let mut rest: Vec<(usize,usize)> = moves.iter()
        .filter(|&&m| !priority.contains(&m) && !near.contains(&m))
        .copied().collect();
    rng.shuffle(&mut rest);

    let mut cands = priority;
    cands.extend_from_slice(&near);
    cands.extend_from_slice(&rest);
    cands.truncate(max_cands);
    if cands.is_empty() { return noob(moves, rng); }

    let mut best = cands[0];
    let mut best_score = -1.0f64;
    for &(r,c) in &cands {
        let score = rollout(game, r, c, player, sims, rng);
        if score > best_score { best_score = score; best = (r,c); }
    }
    best
}

fn rollout(game: &Game, row: usize, col: usize, player: Color, n: usize, rng: &mut Rng) -> f64 {
    let cap = game.board_size() * game.board_size() / 2;
    let mut wins = 0usize;
    for _ in 0..n {
        let mut sim = game.clone();
        if !sim.place_stone(row, col) { continue; }
        let mut steps = 0;
        while !sim.is_game_over() && steps < cap {
            let legal = sim.get_legal_moves();
            if legal.is_empty() { sim.pass_turn(); }
            else {
                let (r,c) = rng.pick(&legal);
                if !sim.place_stone(r,c) { sim.pass_turn(); }
            }
            steps += 1;
        }
        if sim.score_for(player) > sim.score_for(player.opposite()) { wins += 1; }
    }
    wins as f64 / n as f64
}

fn captures_opponent(game: &Game, row: usize, col: usize) -> bool {
    let board = game.board();
    let opp = game.current_player_color().opposite();
    board.neighbors(row,col).iter().any(|&(nr,nc)| {
        board.get(nr,nc) == opp && board.count_liberties(nr,nc) == 1
    })
}

fn saves_atari(game: &Game, row: usize, col: usize) -> bool {
    let board = game.board();
    let cur = game.current_player_color();
    board.neighbors(row,col).iter().any(|&(nr,nc)| {
        board.get(nr,nc) == cur && board.count_liberties(nr,nc) == 1
    })
}
