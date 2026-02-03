use crate::error::Error;
use bit_set::BitSet;
use std::collections::VecDeque;

pub(crate) type Coordinate = (usize, usize);

#[derive(Debug)]
pub(crate) struct Tile {
    adjacent_tiles: BitSet,
    pub(crate) mine: bool,
    pub(crate) exposed: bool,
    pub(crate) flagged: bool,
    pub(crate) adjacent_mines: u8,
}

#[derive(Debug, Copy, Clone, PartialEq)]
enum Increment {
    One,
    NegOne,
    Zero,
}

impl Increment {
    fn offset(&self, value: usize) -> usize {
        match *self {
            Self::One => value + 1,
            Self::NegOne => value.saturating_sub(1),
            Self::Zero => value,
        }
    }
}

fn adjacent((row, column): Coordinate, rows: usize, columns: usize) -> impl Iterator<Item = usize> {
    const INCREMENTS: [Increment; 3] = [Increment::One, Increment::NegOne, Increment::Zero];

    INCREMENTS
        .iter()
        .copied()
        .flat_map(|row_incr| std::iter::repeat(row_incr).zip(INCREMENTS))
        .filter_map(move |(row_incr, column_incr)| {
            let row_offset = row_incr.offset(row);
            let column_offset = column_incr.offset(column);

            if row_offset == row && column_offset == column {
                return None;
            }

            match (row_incr, column_incr) {
                (Increment::Zero, Increment::Zero) => None,
                (_, _) if row_offset < rows && column_offset < columns => {
                    Some(index_from_coord((row_offset, column_offset), columns))
                }
                _ => None,
            }
        })
}

pub(crate) struct Board {
    tiles: Vec<Tile>,
    // number of rows on the board
    pub(crate) rows: usize,
    // number of columns on the board
    pub(crate) columns: usize,
    // the total number of mines
    mines: usize,
    flagged_cells: usize,
    // the total number of correctly flagged mines, allows checking a win in O(1)
    correctly_flagged_mines: usize,
    // the exposed tiles
    seen: BitSet<usize>,
}

fn index_from_coord((r, c): Coordinate, columns: usize) -> usize {
    r * columns + c
}

fn coord_from_index(index: usize, columns: usize) -> Coordinate {
    (index / columns, index % columns)
}

impl Board {
    pub(crate) fn new(rows: usize, columns: usize, mines: usize) -> Result<Self, Error> {
        let mut rng = rand::thread_rng();
        let samples = rand::seq::index::sample(&mut rng, rows * columns, mines)
            .into_iter()
            .collect::<BitSet>();

        let tiles = (0..rows)
            .flat_map(|row| std::iter::repeat(row).zip(0..columns))
            .enumerate()
            .map(|(i, point)| {
                // compute the tiles adjacent to the one being constructed
                let adjacent_tiles = adjacent(point, rows, columns).collect::<BitSet>();

                // sum the number of adjacent tiles that are in the randomly generated mines set
                let adjacent_mines = adjacent_tiles
                    .iter()
                    .fold(0, |total, index| total + u8::from(samples.contains(index)));
                assert!(adjacent_mines <= 8);

                Tile {
                    adjacent_tiles,
                    mine: samples.contains(i),
                    exposed: false,
                    flagged: false,
                    adjacent_mines,
                }
            })
            .collect::<Vec<_>>();

        Ok(Self {
            rows,
            columns,
            tiles,
            mines,
            flagged_cells: Default::default(),
            correctly_flagged_mines: Default::default(),
            seen: Default::default(),
        })
    }

    pub(crate) fn available_flags(&self) -> usize {
        assert!(self.flagged_cells <= self.mines);
        self.mines - self.flagged_cells
    }

    pub(crate) fn won(&self) -> bool {
        let nseen = self.seen.len();
        let exposed_or_correctly_flagged = nseen + self.correctly_flagged_mines;
        let ntiles = self.rows * self.columns;

        assert!(exposed_or_correctly_flagged <= ntiles);

        ntiles == exposed_or_correctly_flagged || (self.tiles.len() - nseen) == self.mines
    }

    fn index_from_coord(&self, (r, c): Coordinate) -> usize {
        index_from_coord((r, c), self.columns)
    }

    pub(crate) fn expose(&mut self, (r, c): Coordinate) -> Result<bool, Error> {
        if self.tile(r, c)?.mine {
            self.tile_mut(r, c)?.exposed = true;
            return Ok(true);
        }

        let mut coordinates = [(r, c)].iter().copied().collect::<VecDeque<_>>();

        let columns = self.columns;

        while let Some((r, c)) = coordinates.pop_front() {
            if self.seen.insert(self.index_from_coord((r, c))) {
                let tile = self.tile_mut(r, c)?;

                tile.exposed = !(tile.mine || tile.flagged);

                if tile.adjacent_mines == 0 {
                    coordinates.extend(
                        tile.adjacent_tiles
                            .iter()
                            .map(move |index| coord_from_index(index, columns)),
                    );
                }
            };
        }

        Ok(false)
    }

    pub(crate) fn expose_all(&mut self) -> Result<(), Error> {
        let columns = self.columns;
        (0..self.tiles.len())
            .map(move |i| coord_from_index(i, columns))
            .try_for_each(|coord| {
                self.expose(coord)?;
                Ok(())
            })
    }

    pub(crate) fn tile(&self, i: usize, j: usize) -> Result<&Tile, Error> {
        self.tiles
            .get(self.index_from_coord((i, j)))
            .ok_or(Error::GetTile((i, j)))
    }

    pub(crate) fn tile_mut(&mut self, i: usize, j: usize) -> Result<&mut Tile, Error> {
        let index = self.index_from_coord((i, j));
        self.tiles.get_mut(index).ok_or(Error::GetTile((i, j)))
    }

    pub(crate) fn flag_all(&mut self) {
        for tile in self.tiles.iter_mut() {
            tile.flagged = !tile.exposed && tile.mine;
        }
    }

    pub(crate) fn flag(&mut self, i: usize, j: usize) -> Result<bool, Error> {
        let nflagged = self.flagged_cells;
        let tile = self.tile(i, j)?;
        let was_flagged = tile.flagged;
        let flagged = !was_flagged;
        let nmines = self.mines;
        self.correctly_flagged_mines += usize::from(flagged && tile.mine);
        if was_flagged {
            self.flagged_cells = self.flagged_cells.saturating_sub(1);
            self.tile_mut(i, j)?.flagged = flagged;
        } else if nflagged < nmines && !self.tile(i, j)?.exposed {
            self.tile_mut(i, j)?.flagged = flagged;
            self.flagged_cells += 1;
        }
        Ok(flagged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_board_initialization() {
        let rows = 10;
        let columns = 10;
        let mines = 10;
        let board = Board::new(rows, columns, mines).unwrap();

        assert_eq!(board.rows, rows);
        assert_eq!(board.columns, columns);
        assert_eq!(board.mines, mines);
        assert_eq!(board.tiles.len(), rows * columns);
    }

    #[test]
    fn test_mine_placement() {
        let rows = 10;
        let columns = 10;
        let mines = 10;
        let board = Board::new(rows, columns, mines).unwrap();

        let mine_count = board.tiles.iter().filter(|t| t.mine).count();
        assert_eq!(mine_count, mines);
    }

    #[test]
    fn test_adjacency() {
        // Create a board with 0 mines to manually check logic if we could controlling randomness,
        // but since we can't easily mock randomness here without changing code,
        // we can check properties.
        // For every tile, calculate adjacent mines and match with what the board says.
        let rows = 5;
        let columns = 5;
        let mines = 5;
        let board = Board::new(rows, columns, mines).unwrap();

        for r in 0..rows {
            for c in 0..columns {
                let tile = board.tile(r, c).unwrap();
                // Manually calculate adjacent mines
                let mut count = 0;
                for dr in -1..=1 {
                    for dc in -1..=1 {
                        if dr == 0 && dc == 0 { continue; }
                        let nr = r as isize + dr;
                        let nc = c as isize + dc;
                        if nr >= 0 && nr < rows as isize && nc >= 0 && nc < columns as isize {
                            if board.tile(nr as usize, nc as usize).unwrap().mine {
                                count += 1;
                            }
                        }
                    }
                }
                assert_eq!(tile.adjacent_mines, count, "Mismatch at ({}, {})", r, c);
            }
        }
    }

    #[test]
    fn test_expose() {
        let rows = 5;
        let columns = 5;
        let mines = 0; // 0 mines means safe expose everywhere
        let mut board = Board::new(rows, columns, mines).unwrap();

        // Expose top left
        board.expose((0, 0)).unwrap();

        // Since 0 mines, exposing one should expose all (flood fill)
        let exposed_count = board.tiles.iter().filter(|t| t.exposed).count();
        assert_eq!(exposed_count, rows * columns);
    }

    #[test]
    fn test_win_condition() {
        let rows = 3;
        let columns = 3;
        let mines = 1;
        let mut board = Board::new(rows, columns, mines).unwrap();

        // Find the mine
        let mut mine_coord = (0, 0);
        for r in 0..rows {
            for c in 0..columns {
                if board.tile(r, c).unwrap().mine {
                    mine_coord = (r, c);
                    break;
                }
            }
        }

        // Expose all non-mine cells
        for r in 0..rows {
            for c in 0..columns {
                if (r, c) != mine_coord {
                    board.expose((r, c)).unwrap();
                }
            }
        }

        assert!(board.won());
    }

    #[test]
    fn test_flagging() {
        let rows = 5;
        let columns = 5;
        let mines = 5;
        let mut board = Board::new(rows, columns, mines).unwrap();

        // Flag a cell
        let flags_before = board.flagged_cells;
        board.flag(0, 0).unwrap();
        assert_eq!(board.flagged_cells, flags_before + 1);
        assert!(board.tile(0, 0).unwrap().flagged);

        // Unflag
        board.flag(0, 0).unwrap();
        assert_eq!(board.flagged_cells, flags_before);
        assert!(!board.tile(0, 0).unwrap().flagged);
    }
}
