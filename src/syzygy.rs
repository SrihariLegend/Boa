// ============================================================
// syzygy.rs - Syzygy tablebase integration
// ============================================================

use crate::board::{Board, Zobrist};
use crate::config::SyzygyOptions;
use crate::movegen::{gen_moves, AttackTables};
use crate::types::*;
use shakmaty::{fen::Fen, CastlingMode, Chess};
use shakmaty_syzygy::{AmbiguousWdl, Tablebase, Wdl};
use std::{io, path::Path};

pub const TB_WIN_SCORE: Score = 800_000;
const TB_CURSED_WIN_SCORE: Score = 10;

pub struct SyzygyTablebase {
    tables: Tablebase<Chess>,
    files: usize,
}

pub struct SyzygyRootProbe {
    pub best_move: Move,
    pub score: Score,
    pub wdl: String,
    pub dtz: i32,
}

impl SyzygyTablebase {
    pub fn load(path_list: &str) -> Result<Option<Self>, String> {
        let trimmed = path_list.trim();
        if trimmed.is_empty() || trimmed == "<empty>" {
            return Ok(None);
        }

        let mut tables = Tablebase::new();
        let mut files = 0usize;
        for path in split_syzygy_paths(trimmed) {
            if path.as_os_str().is_empty() {
                continue;
            }
            files += add_directory(&mut tables, &path)?;
        }

        if files == 0 {
            return Err("no Syzygy table files found".to_string());
        }

        Ok(Some(SyzygyTablebase { tables, files }))
    }

    pub fn file_count(&self) -> usize {
        self.files
    }

    pub fn max_pieces(&self) -> usize {
        self.tables.max_pieces()
    }

    pub fn probe_score(
        &self,
        board: &Board,
        options: &SyzygyOptions,
        depth: i32,
        ply: usize,
    ) -> Option<Score> {
        if !can_probe(board, options, self.max_pieces(), depth) {
            return None;
        }
        let pos = board_to_shakmaty(board)?;
        probe_position_score(&self.tables, &pos, options, ply)
    }

    pub fn probe_root(
        &self,
        board: &Board,
        atk: &AttackTables,
        z: &Zobrist,
        options: &SyzygyOptions,
    ) -> Option<SyzygyRootProbe> {
        if !can_probe(
            board,
            options,
            self.max_pieces(),
            options.probe_depth as i32,
        ) {
            return None;
        }

        let pos = board_to_shakmaty(board)?;
        let (shak_move, dtz) = self.tables.best_move(&pos).ok()??;
        let uci = shakmaty::uci::UciMove::from_standard(shak_move).to_string();
        let parsed = move_from_uci(&uci)?;
        let best_move = find_legal_move(board, atk, z, parsed)?;
        let score = probe_position_score(&self.tables, &pos, options, 0)?;

        Some(SyzygyRootProbe {
            best_move,
            score,
            wdl: score_wdl_name(score).to_string(),
            dtz: dtz.ignore_rounding().0,
        })
    }
}

fn add_directory(tables: &mut Tablebase<Chess>, path: &Path) -> Result<usize, String> {
    let entries = std::fs::read_dir(path)
        .map_err(|err| format!("failed to read Syzygy path {}: {err}", path.display()))?;
    let mut files = 0usize;

    for entry in entries {
        let entry =
            entry.map_err(|err| format!("failed to read Syzygy path {}: {err}", path.display()))?;
        match tables.add_file(entry.path()) {
            Ok(()) => files += 1,
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData
                ) =>
            {
                continue;
            }
            Err(err) => {
                return Err(format!(
                    "failed to load Syzygy file {}: {err}",
                    entry.path().display()
                ));
            }
        }
    }

    Ok(files)
}

fn split_syzygy_paths(path_list: &str) -> Vec<std::path::PathBuf> {
    if path_list.contains(';') {
        return path_list
            .split(';')
            .filter(|path| !path.trim().is_empty())
            .map(std::path::PathBuf::from)
            .collect();
    }
    std::env::split_paths(path_list).collect()
}

fn can_probe(board: &Board, options: &SyzygyOptions, available_pieces: usize, depth: i32) -> bool {
    if options.probe_limit == 0 || depth < options.probe_depth as i32 {
        return false;
    }
    if board.castling != 0 {
        return false;
    }
    let pieces = bb_popcount(board.occ_all) as usize;
    pieces >= 2 && pieces <= options.probe_limit.min(available_pieces)
}

fn board_to_shakmaty(board: &Board) -> Option<Chess> {
    board
        .to_fen()
        .parse::<Fen>()
        .ok()?
        .into_position(CastlingMode::Standard)
        .ok()
}

fn probe_position_score(
    tables: &Tablebase<Chess>,
    pos: &Chess,
    options: &SyzygyOptions,
    ply: usize,
) -> Option<Score> {
    if options.fifty_move_rule {
        return tables
            .probe_wdl(pos)
            .ok()
            .map(|wdl| ambiguous_wdl_score(wdl, ply));
    }
    tables
        .probe_wdl_after_zeroing(pos)
        .ok()
        .map(|wdl| wdl_score(wdl, ply))
}

fn wdl_score(wdl: Wdl, ply: usize) -> Score {
    match wdl {
        Wdl::Win => TB_WIN_SCORE - ply as Score,
        Wdl::CursedWin => TB_CURSED_WIN_SCORE,
        Wdl::Draw => SCORE_DRAW,
        Wdl::BlessedLoss => -TB_CURSED_WIN_SCORE,
        Wdl::Loss => -TB_WIN_SCORE + ply as Score,
    }
}

fn ambiguous_wdl_score(wdl: AmbiguousWdl, ply: usize) -> Score {
    if let Some(unambiguous) = wdl.unambiguous() {
        return wdl_score(unambiguous, ply);
    }
    match wdl.signum() {
        1 => TB_CURSED_WIN_SCORE,
        -1 => -TB_CURSED_WIN_SCORE,
        _ => SCORE_DRAW,
    }
}

fn score_wdl_name(score: Score) -> &'static str {
    if score >= TB_WIN_SCORE / 2 {
        "win"
    } else if score <= -TB_WIN_SCORE / 2 {
        "loss"
    } else if score > 0 {
        "cursed-win"
    } else if score < 0 {
        "blessed-loss"
    } else {
        "draw"
    }
}

fn find_legal_move(board: &Board, atk: &AttackTables, z: &Zobrist, m: Move) -> Option<Move> {
    let from = move_from(m);
    let to = move_to(m);
    let promo_flag = move_flags(m) == MF_PROMOTION;
    let list = gen_moves(board, atk);

    for &legal_move in list.iter() {
        if move_from(legal_move) != from || move_to(legal_move) != to {
            continue;
        }
        if promo_flag
            && (move_flags(legal_move) != MF_PROMOTION
                || move_promo_pt(legal_move) != move_promo_pt(m))
        {
            continue;
        }

        let mut clone = board.clone();
        let undo = clone.make_move(legal_move, z);
        let legal = !clone.is_in_check(clone.side.flip());
        clone.unmake_move(legal_move, &undo, z);
        if legal {
            return Some(legal_move);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_path_disables_tablebases() {
        assert!(SyzygyTablebase::load("").unwrap().is_none());
        assert!(SyzygyTablebase::load("<empty>").unwrap().is_none());
    }

    #[test]
    fn probe_limit_caps_at_six_pieces() {
        let mut options = SyzygyOptions::default();
        options.probe_limit = 6;
        let board = Board::from_fen("8/8/8/8/8/8/4K3/4k3 w - - 0 1").unwrap();
        assert!(can_probe(&board, &options, 6, 1));
    }
}
