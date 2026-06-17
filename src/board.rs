pub const SIZE: usize = 19;

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
    pub cells: [[Color; SIZE]; SIZE],
}

impl Board {
    pub fn new() -> Self {
        Board {
            cells: [[Color::Empty; SIZE]; SIZE],
        }
    }

    pub fn get(&self, row: usize, col: usize) -> Color {
        self.cells[row][col]
    }

    pub fn set(&mut self, row: usize, col: usize, color: Color) {
        self.cells[row][col] = color;
    }

    pub fn neighbors(row: usize, col: usize) -> Vec<(usize, usize)> {
        let mut result = Vec::new();
        if row > 0 { result.push((row - 1, col)); }
        if row < SIZE - 1 { result.push((row + 1, col)); }
        if col > 0 { result.push((row, col - 1)); }
        if col < SIZE - 1 { result.push((row, col + 1)); }
        result
    }

    pub fn get_group(&self, row: usize, col: usize) -> Vec<(usize, usize)> {
        let color = self.get(row, col);
        if color == Color::Empty {
            return vec![];
        }
        let mut visited = [[false; SIZE]; SIZE];
        let mut group = Vec::new();
        self.collect_group(row, col, color, &mut visited, &mut group);
        group
    }

    fn collect_group(
        &self,
        row: usize,
        col: usize,
        color: Color,
        visited: &mut [[bool; SIZE]; SIZE],
        group: &mut Vec<(usize, usize)>,
    ) {
        if visited[row][col] || self.get(row, col) != color {
            return;
        }
        visited[row][col] = true;
        group.push((row, col));
        for (nr, nc) in Self::neighbors(row, col) {
            self.collect_group(nr, nc, color, visited, group);
        }
    }

    pub fn group_has_liberty(&self, row: usize, col: usize) -> bool {
        let group = self.get_group(row, col);
        group.iter().any(|&(r, c)| {
            Self::neighbors(r, c)
                .iter()
                .any(|&(nr, nc)| self.get(nr, nc) == Color::Empty)
        })
    }

    pub fn count_liberties(&self, row: usize, col: usize) -> usize {
        let group = self.get_group(row, col);
        let mut liberties = std::collections::HashSet::new();
        for (r, c) in group {
            for (nr, nc) in Self::neighbors(r, c) {
                if self.get(nr, nc) == Color::Empty {
                    liberties.insert((nr, nc));
                }
            }
        }
        liberties.len()
    }

    /// Remove all stones of `color` that have no liberties. Returns count removed.
    pub fn remove_captured(&mut self, color: Color) -> usize {
        let mut to_remove: Vec<(usize, usize)> = Vec::new();
        let mut checked = [[false; SIZE]; SIZE];

        for r in 0..SIZE {
            for c in 0..SIZE {
                if !checked[r][c] && self.get(r, c) == color {
                    let group = self.get_group(r, c);
                    for &(gr, gc) in &group {
                        checked[gr][gc] = true;
                    }
                    let has_liberty = group.iter().any(|&(gr, gc)| {
                        Self::neighbors(gr, gc)
                            .iter()
                            .any(|&(nr, nc)| self.get(nr, nc) == Color::Empty)
                    });
                    if !has_liberty {
                        to_remove.extend_from_slice(&group);
                    }
                }
            }
        }

        let count = to_remove.len();
        for (r, c) in to_remove {
            self.set(r, c, Color::Empty);
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_board_is_empty() {
        let board = Board::new();
        for r in 0..SIZE {
            for c in 0..SIZE {
                assert_eq!(board.get(r, c), Color::Empty);
            }
        }
    }

    #[test]
    fn neighbors_corner_has_two() {
        let n = Board::neighbors(0, 0);
        assert_eq!(n.len(), 2);
        assert!(n.contains(&(0, 1)));
        assert!(n.contains(&(1, 0)));
    }

    #[test]
    fn neighbors_edge_has_three() {
        let n = Board::neighbors(0, 5);
        assert_eq!(n.len(), 3);
    }

    #[test]
    fn neighbors_center_has_four() {
        let n = Board::neighbors(9, 9);
        assert_eq!(n.len(), 4);
    }

    #[test]
    fn single_stone_center_has_four_liberties() {
        let mut board = Board::new();
        board.set(9, 9, Color::Black);
        assert_eq!(board.count_liberties(9, 9), 4);
    }

    #[test]
    fn single_stone_corner_has_two_liberties() {
        let mut board = Board::new();
        board.set(0, 0, Color::Black);
        assert_eq!(board.count_liberties(0, 0), 2);
    }

    #[test]
    fn capture_removes_surrounded_single_stone() {
        let mut board = Board::new();
        board.set(1, 1, Color::White);
        board.set(0, 1, Color::Black);
        board.set(2, 1, Color::Black);
        board.set(1, 0, Color::Black);
        board.set(1, 2, Color::Black);
        let removed = board.remove_captured(Color::White);
        assert_eq!(removed, 1);
        assert_eq!(board.get(1, 1), Color::Empty);
    }

    #[test]
    fn no_capture_when_liberty_remains() {
        let mut board = Board::new();
        board.set(1, 1, Color::White);
        board.set(0, 1, Color::Black);
        board.set(2, 1, Color::Black);
        board.set(1, 0, Color::Black);
        // (1,2) is still empty — white survives
        let removed = board.remove_captured(Color::White);
        assert_eq!(removed, 0);
        assert_eq!(board.get(1, 1), Color::White);
    }

    #[test]
    fn capture_removes_entire_group() {
        let mut board = Board::new();
        // Two-stone white group at (1,1) and (1,2)
        board.set(1, 1, Color::White);
        board.set(1, 2, Color::White);
        // Surround the group
        board.set(0, 1, Color::Black);
        board.set(0, 2, Color::Black);
        board.set(2, 1, Color::Black);
        board.set(2, 2, Color::Black);
        board.set(1, 0, Color::Black);
        board.set(1, 3, Color::Black);
        let removed = board.remove_captured(Color::White);
        assert_eq!(removed, 2);
        assert_eq!(board.get(1, 1), Color::Empty);
        assert_eq!(board.get(1, 2), Color::Empty);
    }
}
