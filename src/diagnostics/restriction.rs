use super::*;
pub(in crate::diagnostics) const LIBERATING_MOBILITY_GAIN: u32 = 5;
pub(in crate::diagnostics) const REDEPLOYMENT_MOBILITY_GAIN: u32 = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestrictionFeatures {
    pub fen: String,
    pub side_to_move: Color,
    pub static_eval_cp: i32,
    pub white_score_cp: i32,
    pub phase: i32,
    pub material_cp: i32,
    pub pst_cp: i32,
    pub mobility_cp: i32,
    pub pawn_structure_cp: i32,
    pub king_safety_cp: i32,
    pub material_mg: i32,
    pub material_eg: i32,
    pub pst_mg: i32,
    pub pst_eg: i32,
    pub mobility_white: u32,
    pub mobility_black: u32,
    pub mobility_mg: i32,
    pub mobility_eg: i32,
    pub pawn_structure_mg: i32,
    pub pawn_structure_eg: i32,
    pub king_safety_mg: i32,
    pub king_safety_eg: i32,
    pub white_pawn_breaks: u32,
    pub black_pawn_breaks: u32,
    pub liberating_breaks_white: u32,
    pub liberating_breaks_black: u32,
    pub piece_redeployment_white: u32,
    pub piece_redeployment_black: u32,
}

impl RestrictionFeatures {
    pub fn csv_header() -> &'static str {
        "fen,side_to_move,static_eval_cp,white_score_cp,phase,\
         material_cp,pst_cp,mobility_cp,pawn_structure_cp,king_safety_cp,\
         material_mg,material_eg,pst_mg,pst_eg,\
         mobility_white,mobility_black,mobility_mg,mobility_eg,\
         pawn_structure_mg,pawn_structure_eg,king_safety_mg,king_safety_eg,\
         white_pawn_breaks,black_pawn_breaks,liberating_breaks_white,liberating_breaks_black,\
         piece_redeployment_white,piece_redeployment_black"
    }

    pub fn to_csv_row(&self) -> String {
        [
            csv_string(&self.fen),
            color_name(self.side_to_move).to_string(),
            self.static_eval_cp.to_string(),
            self.white_score_cp.to_string(),
            self.phase.to_string(),
            self.material_cp.to_string(),
            self.pst_cp.to_string(),
            self.mobility_cp.to_string(),
            self.pawn_structure_cp.to_string(),
            self.king_safety_cp.to_string(),
            self.material_mg.to_string(),
            self.material_eg.to_string(),
            self.pst_mg.to_string(),
            self.pst_eg.to_string(),
            self.mobility_white.to_string(),
            self.mobility_black.to_string(),
            self.mobility_mg.to_string(),
            self.mobility_eg.to_string(),
            self.pawn_structure_mg.to_string(),
            self.pawn_structure_eg.to_string(),
            self.king_safety_mg.to_string(),
            self.king_safety_eg.to_string(),
            self.white_pawn_breaks.to_string(),
            self.black_pawn_breaks.to_string(),
            self.liberating_breaks_white.to_string(),
            self.liberating_breaks_black.to_string(),
            self.piece_redeployment_white.to_string(),
            self.piece_redeployment_black.to_string(),
        ]
        .join(",")
    }
}

pub fn extract_restriction_features(
    board: &Board,
    atk: &AttackTables,
    z: &Zobrist,
    options: EngineOptions,
) -> RestrictionFeatures {
    let pawn_cache = std::cell::RefCell::new(crate::eval::PawnEvalCache::new());
    let ctx = EvalContext {
        atk,
        options: &options,
        pawn_cache: &pawn_cache,
    };
    let eval = evaluate_breakdown(board, &ctx);
    let white_breaks = count_pawn_breaks(board, atk, z, Color::White);
    let black_breaks = count_pawn_breaks(board, atk, z, Color::Black);

    RestrictionFeatures {
        fen: board.to_fen(),
        side_to_move: board.side,
        static_eval_cp: eval.side_to_move_score,
        white_score_cp: eval.white_score,
        phase: eval.phase,
        material_cp: eval.material_cp,
        pst_cp: eval.pst_cp,
        mobility_cp: eval.mobility_cp,
        pawn_structure_cp: eval.pawn_structure_cp,
        king_safety_cp: eval.king_safety_cp,
        material_mg: eval.material_mg,
        material_eg: eval.material_eg,
        pst_mg: eval.pst_mg,
        pst_eg: eval.pst_eg,
        mobility_white: eval.mobility_white,
        mobility_black: eval.mobility_black,
        mobility_mg: eval.mobility_mg,
        mobility_eg: eval.mobility_eg,
        pawn_structure_mg: eval.pawn_structure_mg,
        pawn_structure_eg: eval.pawn_structure_eg,
        king_safety_mg: eval.king_safety_mg,
        king_safety_eg: eval.king_safety_eg,
        white_pawn_breaks: white_breaks.total,
        black_pawn_breaks: black_breaks.total,
        liberating_breaks_white: white_breaks.liberating,
        liberating_breaks_black: black_breaks.liberating,
        piece_redeployment_white: count_piece_redeployments(board, atk, z, Color::White),
        piece_redeployment_black: count_piece_redeployments(board, atk, z, Color::Black),
    }
}
