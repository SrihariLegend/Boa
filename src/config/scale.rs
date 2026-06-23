pub fn scale_score(score: i32, scale: i32) -> i32 {
    score * scale / 100
}

pub fn scale_score_pair(score: (i32, i32), scale: i32) -> (i32, i32) {
    (scale_score(score.0, scale), scale_score(score.1, scale))
}
