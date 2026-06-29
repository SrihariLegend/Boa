use super::*;
use crate::probe;
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
        let result = probe_position_score(&self.tables, &pos, options, ply);
        if let Some(score) = result {
            probe!(Syzygy, SyzygyEvent {
                result: if score > 0 { "win" } else if score < 0 { "loss" } else { "draw" },
                distance_to_mate: if score.abs() > SCORE_MATE - 128 { (SCORE_MATE - score.abs()) as i32 } else { 0 },
                piece_count: 0, // TODO: compute from board occupancy
                dtz_value: 0,
                wdl_probe_success: true,
            });
        }
        result
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
