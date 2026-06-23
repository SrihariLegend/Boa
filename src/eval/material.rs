use super::*;
pub(in crate::eval) fn compute_phase(board: &Board) -> i32 {
    let w = board.non_pawn_material(Color::White);
    let b = board.non_pawn_material(Color::Black);
    game_phase(w + b)
}

pub(in crate::eval) fn material_and_pst(board: &Board) -> ((i32, i32), (i32, i32)) {
    let mut mat_mg = 0i32;
    let mut mat_eg = 0i32;
    let mut pst_mg_total = 0i32;
    let mut pst_eg_total = 0i32;
    for c in [Color::White, Color::Black] {
        let sign = if c == Color::White { 1 } else { -1 };
        let ci = c as usize;
        for pt_u8 in 0..6u8 {
            let pt = PieceType::from_u8(pt_u8);
            let mut pieces = board.pieces[ci][pt_u8 as usize];
            while pieces != 0 {
                let sq = bb_pop_lsb(&mut pieces);
                let mat = pt.material_value();
                let (pst_mg, pst_eg) = pst_value(pt, sq, c);
                mat_mg += sign * mat;
                mat_eg += sign * mat;
                pst_mg_total += sign * pst_mg;
                pst_eg_total += sign * pst_eg;
            }
        }
    }
    ((mat_mg, mat_eg), (pst_mg_total, pst_eg_total))
}
