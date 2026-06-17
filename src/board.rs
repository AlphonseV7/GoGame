#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Color {
    Empty,
    Black,
    White,
}

impl Color {
    pub fn opposite(self) -> Self {
        match self {
            Color::Black => Color::White,
            Color::White => Color::Black,
            Color::Empty => Color::Empty,
        }
    }
}

#[derive(Clone)]
pub struct Board {
    size: usize,
    cells: Vec<Color>,
}

impl Board {
    pub fn new(size: usize) -> Self {
        Board { size, cells: vec![Color::Empty; size * size] }
    }

    pub fn size(&self) -> usize { self.size }

    fn idx(&self, row: usize, col: usize) -> usize { row * self.size + col }

    pub fn get(&self, row: usize, col: usize) -> Color { self.cells[self.idx(row, col)] }

    pub fn set(&mut self, row: usize, col: usize, color: Color) {
        let i = self.idx(row, col);
        self.cells[i] = color;
    }

    pub fn neighbors(&self, row: usize, col: usize) -> Vec<(usize, usize)> {
        let mut v = Vec::new();
        if row > 0            { v.push((row - 1, col)); }
        if row < self.size-1  { v.push((row + 1, col)); }
        if col > 0            { v.push((row, col - 1)); }
        if col < self.size-1  { v.push((row, col + 1)); }
        v
    }

    pub fn get_group(&self, row: usize, col: usize) -> Vec<(usize, usize)> {
        let color = self.get(row, col);
        if color == Color::Empty { return vec![]; }
        let mut visited = vec![false; self.size * self.size];
        let mut group = Vec::new();
        self.collect_group(row, col, color, &mut visited, &mut group);
        group
    }

    fn collect_group(
        &self, row: usize, col: usize, color: Color,
        visited: &mut Vec<bool>, group: &mut Vec<(usize, usize)>,
    ) {
        let i = self.idx(row, col);
        if visited[i] || self.get(row, col) != color { return; }
        visited[i] = true;
        group.push((row, col));
        for (nr, nc) in self.neighbors(row, col) {
            self.collect_group(nr, nc, color, visited, group);
        }
    }

    pub fn group_has_liberty(&self, row: usize, col: usize) -> bool {
        self.get_group(row, col).iter().any(|&(r, c)| {
            self.neighbors(r, c).iter().any(|&(nr, nc)| self.get(nr, nc) == Color::Empty)
        })
    }

    pub fn count_liberties(&self, row: usize, col: usize) -> usize {
        let mut libs = std::collections::HashSet::new();
        for (r, c) in self.get_group(row, col) {
            for (nr, nc) in self.neighbors(r, c) {
                if self.get(nr, nc) == Color::Empty { libs.insert((nr, nc)); }
            }
        }
        libs.len()
    }

    pub fn remove_captured(&mut self, color: Color) -> usize {
        let size = self.size;
        let mut to_remove = Vec::new();
        let mut checked = vec![false; size * size];
        for r in 0..size {
            for c in 0..size {
                let i = r * size + c;
                if !checked[i] && self.get(r, c) == color {
                    let group = self.get_group(r, c);
                    for &(gr, gc) in &group { checked[gr * size + gc] = true; }
                    let alive = group.iter().any(|&(gr, gc)| {
                        self.neighbors(gr, gc).iter().any(|&(nr, nc)| self.get(nr, nc) == Color::Empty)
                    });
                    if !alive { to_remove.extend_from_slice(&group); }
                }
            }
        }
        let count = to_remove.len();
        for (r, c) in to_remove { self.set(r, c, Color::Empty); }
        count
    }

    pub fn count_stones(&self, color: Color) -> usize {
        self.cells.iter().filter(|&&c| c == color).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_board_is_empty() {
        let b = Board::new(19);
        for r in 0..19 { for c in 0..19 { assert_eq!(b.get(r, c), Color::Empty); } }
    }

    #[test]
    fn neighbors_corner_has_two() {
        let b = Board::new(19);
        let n = b.neighbors(0, 0);
        assert_eq!(n.len(), 2);
        assert!(n.contains(&(0,1)) && n.contains(&(1,0)));
    }

    #[test]
    fn neighbors_edge_has_three() {
        assert_eq!(Board::new(19).neighbors(0, 5).len(), 3);
    }

    #[test]
    fn neighbors_center_has_four() {
        assert_eq!(Board::new(19).neighbors(9, 9).len(), 4);
    }

    #[test]
    fn single_stone_center_liberties() {
        let mut b = Board::new(19);
        b.set(9, 9, Color::Black);
        assert_eq!(b.count_liberties(9, 9), 4);
    }

    #[test]
    fn single_stone_corner_liberties() {
        let mut b = Board::new(19);
        b.set(0, 0, Color::Black);
        assert_eq!(b.count_liberties(0, 0), 2);
    }

    #[test]
    fn capture_single_stone() {
        let mut b = Board::new(19);
        b.set(1,1,Color::White);
        b.set(0,1,Color::Black); b.set(2,1,Color::Black);
        b.set(1,0,Color::Black); b.set(1,2,Color::Black);
        assert_eq!(b.remove_captured(Color::White), 1);
        assert_eq!(b.get(1,1), Color::Empty);
    }

    #[test]
    fn no_capture_with_liberty() {
        let mut b = Board::new(19);
        b.set(1,1,Color::White);
        b.set(0,1,Color::Black); b.set(2,1,Color::Black); b.set(1,0,Color::Black);
        assert_eq!(b.remove_captured(Color::White), 0);
    }

    #[test]
    fn capture_two_stone_group() {
        let mut b = Board::new(19);
        b.set(1,1,Color::White); b.set(1,2,Color::White);
        b.set(0,1,Color::Black); b.set(0,2,Color::Black);
        b.set(2,1,Color::Black); b.set(2,2,Color::Black);
        b.set(1,0,Color::Black); b.set(1,3,Color::Black);
        assert_eq!(b.remove_captured(Color::White), 2);
    }

    #[test]
    fn board_9x9_works() {
        let mut b = Board::new(9);
        b.set(4,4,Color::Black);
        assert_eq!(b.count_liberties(4,4), 4);
        assert_eq!(b.neighbors(0,0).len(), 2);
        assert_eq!(b.neighbors(8,8).len(), 2);
    }
}
